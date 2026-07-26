import { invoke } from '@tauri-apps/api/core'
import type { CreatePublicLinkOptions, PublicLink, PublicLinkKind } from '../types/backend'

export const publicLinks = {
  list(mountId: string) {
    return invoke<PublicLink[]>('list_public_links', { mountId })
  },

  create(mountId: string, path: string, options: CreatePublicLinkOptions) {
    return invoke<PublicLink>('create_public_link', { mountId, path, options })
  },

  remove(mountId: string, linkId: string, kind: PublicLinkKind) {
    return invoke<void>('delete_public_link', { mountId, linkId, kind })
  },
}
