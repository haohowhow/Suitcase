import tseslint from 'typescript-eslint'

export default tseslint.config(
  {
    ignores: ['dist', 'node_modules', 'public/pdfjs', 'src-tauri/target', '.omo', '.claude', '.codex-tmp', '.codegraph'],
  },
  ...tseslint.configs.recommended,
)
