use std::{
    ffi::{c_int, c_uchar, c_ushort, c_void},
    io::Cursor,
    sync::Mutex,
};

use image::{DynamicImage, ImageFormat, ImageReader, RgbImage};

use crate::error::AppError;

const MAX_PREVIEW_PIXELS: u64 = 80_000_000;
/// Embedded previews smaller than this on their long edge look blurry in the
/// viewer; developing the RAW file is worth the extra time then.
const MIN_EMBEDDED_PREVIEW_EDGE: u32 = 1024;
const LIBRAW_SUCCESS: c_int = 0;
const LIBRAW_IMAGE_JPEG: c_int = 1;
const LIBRAW_IMAGE_BITMAP: c_int = 2;
static RAW_DECODE_LOCK: Mutex<()> = Mutex::new(());

#[repr(C)]
struct LibRawProcessedImage {
    image_type: c_int,
    height: c_ushort,
    width: c_ushort,
    colors: c_ushort,
    bits: c_ushort,
    data_size: u32,
    data: [c_uchar; 1],
}

#[link(name = "libraw_static", kind = "static")]
unsafe extern "C" {
    fn libraw_init(flags: c_int) -> *mut c_void;
    fn libraw_close(data: *mut c_void);
    fn libraw_open_buffer(data: *mut c_void, buffer: *const c_void, size: usize) -> c_int;
    fn libraw_unpack(data: *mut c_void) -> c_int;
    fn libraw_unpack_thumb(data: *mut c_void) -> c_int;
    fn libraw_dcraw_process(data: *mut c_void) -> c_int;
    fn libraw_dcraw_make_mem_thumb(
        data: *mut c_void,
        error_code: *mut c_int,
    ) -> *mut LibRawProcessedImage;
    fn libraw_dcraw_make_mem_image(
        data: *mut c_void,
        error_code: *mut c_int,
    ) -> *mut LibRawProcessedImage;
    fn libraw_dcraw_clear_mem(image: *mut LibRawProcessedImage);
    fn libraw_set_half_size(data: *mut c_void, value: c_int);
    fn libraw_set_use_camera_wb(data: *mut c_void, value: c_int);
    fn libraw_set_demosaic(data: *mut c_void, value: c_int);
    fn libraw_set_output_bps(data: *mut c_void, value: c_int);
    fn libraw_set_output_color(data: *mut c_void, value: c_int);
}

struct LibRawGuard(*mut c_void);

impl Drop for LibRawGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer was returned by libraw_init and is owned by this guard.
            unsafe { libraw_close(self.0) };
        }
    }
}

pub fn decode_tiff(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Tiff)
        .into_dimensions()
        .map_err(|_| AppError::PreviewDecode)?;
    validate_pixels(width, height)?;
    let image = image::load_from_memory_with_format(bytes, ImageFormat::Tiff)
        .map_err(|_| AppError::PreviewDecode)?;
    encode_bounded_png(image)
}

pub fn decode_raw(bytes: &[u8]) -> Result<(Vec<u8>, String), AppError> {
    let _serial = RAW_DECODE_LOCK
        .lock()
        .map_err(|_| AppError::PreviewDecode)?;
    // SAFETY: flags=0 is LibRaw's default initialization.
    let data = unsafe { libraw_init(0) };
    if data.is_null() {
        return Err(AppError::PreviewDecode);
    }
    let guard = LibRawGuard(data);
    // SAFETY: bytes remains alive and immutable for the lifetime of this LibRaw instance.
    if unsafe { libraw_open_buffer(guard.0, bytes.as_ptr().cast(), bytes.len()) } != LIBRAW_SUCCESS
    {
        return Err(AppError::PreviewDecode);
    }

    // Prefer an embedded preview to avoid demosaicing a full sensor frame.
    if unsafe { libraw_unpack_thumb(guard.0) } == LIBRAW_SUCCESS
        && let Ok(result) = make_processed_image(guard.0, true)
    {
        return Ok(result);
    }

    // Fall back to a half-size, 8-bit sRGB development.
    unsafe {
        libraw_set_half_size(guard.0, 1);
        libraw_set_use_camera_wb(guard.0, 1);
        libraw_set_demosaic(guard.0, 0);
        libraw_set_output_bps(guard.0, 8);
        libraw_set_output_color(guard.0, 1);
    }
    if unsafe { libraw_unpack(guard.0) } != LIBRAW_SUCCESS
        || unsafe { libraw_dcraw_process(guard.0) } != LIBRAW_SUCCESS
    {
        return Err(AppError::PreviewDecode);
    }
    make_processed_image(guard.0, false)
}

fn make_processed_image(data: *mut c_void, thumbnail: bool) -> Result<(Vec<u8>, String), AppError> {
    let mut error_code = 0;
    // SAFETY: data is an opened and appropriately unpacked LibRaw instance.
    let image_ptr = unsafe {
        if thumbnail {
            libraw_dcraw_make_mem_thumb(data, &mut error_code)
        } else {
            libraw_dcraw_make_mem_image(data, &mut error_code)
        }
    };
    if image_ptr.is_null() || error_code != LIBRAW_SUCCESS {
        return Err(AppError::PreviewDecode);
    }
    let result = unsafe {
        let image = &*image_ptr;
        let bytes =
            std::slice::from_raw_parts(image.data.as_ptr(), image.data_size as usize).to_vec();
        match image.image_type {
            // LibRaw leaves width and height at zero for JPEG previews, so the size
            // has to come from the JPEG header itself.
            LIBRAW_IMAGE_JPEG => {
                accept_embedded_jpeg(&bytes).map(|()| (bytes, "image/jpeg".to_owned()))
            }
            LIBRAW_IMAGE_BITMAP if image.colors == 3 && image.bits == 8 => {
                validate_pixels(u32::from(image.width), u32::from(image.height)).and_then(|()| {
                    let rgb =
                        RgbImage::from_raw(u32::from(image.width), u32::from(image.height), bytes)
                            .ok_or(AppError::PreviewDecode)?;
                    encode_bounded_png(DynamicImage::ImageRgb8(rgb))
                        .map(|png| (png, "image/png".to_owned()))
                })
            }
            _ => Err(AppError::PreviewDecode),
        }
    };
    // SAFETY: image_ptr was allocated by libraw_dcraw_make_mem_* above.
    unsafe { libraw_dcraw_clear_mem(image_ptr) };
    result
}

/// Accepts an embedded JPEG preview that is a real JPEG, within the pixel limit
/// and large enough to be worth showing instead of developing the RAW data.
fn accept_embedded_jpeg(bytes: &[u8]) -> Result<(), AppError> {
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
        .into_dimensions()
        .map_err(|_| AppError::PreviewDecode)?;
    validate_pixels(width, height)?;
    if width.max(height) < MIN_EMBEDDED_PREVIEW_EDGE {
        return Err(AppError::PreviewDecode);
    }
    Ok(())
}

fn encode_bounded_png(image: DynamicImage) -> Result<Vec<u8>, AppError> {
    validate_pixels(image.width(), image.height())?;
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| AppError::PreviewDecode)?;
    Ok(output.into_inner())
}

fn validate_pixels(width: u32, height: u32) -> Result<(), AppError> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > MAX_PREVIEW_PIXELS {
        return Err(AppError::PreviewTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_tiff_and_raw_are_rejected() {
        assert!(matches!(
            decode_tiff(b"not-a-tiff"),
            Err(AppError::PreviewDecode)
        ));
        assert!(matches!(
            decode_raw(b"not-a-raw"),
            Err(AppError::PreviewDecode)
        ));
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(RgbImage::new(width, height))
            .write_to(&mut output, ImageFormat::Jpeg)
            .expect("encode test JPEG");
        output.into_inner()
    }

    #[test]
    fn uses_an_embedded_jpeg_only_when_it_is_large_enough() {
        assert!(accept_embedded_jpeg(&jpeg(1600, 1067)).is_ok());
        assert!(accept_embedded_jpeg(&jpeg(800, 1200)).is_ok());
        assert!(matches!(
            accept_embedded_jpeg(&jpeg(160, 120)),
            Err(AppError::PreviewDecode)
        ));
        assert!(matches!(
            accept_embedded_jpeg(b"not a jpeg"),
            Err(AppError::PreviewDecode)
        ));
    }

    #[test]
    fn rejects_images_above_the_eighty_megapixel_limit() {
        assert!(validate_pixels(10_000, 8_000).is_ok());
        assert!(matches!(
            validate_pixels(10_001, 8_000),
            Err(AppError::PreviewTooLarge)
        ));
    }
}
