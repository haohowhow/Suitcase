import { StrictMode, type ComponentType } from 'react'
import { createRoot } from 'react-dom/client'
import './styles.css'

if (import.meta.env.DEV) {
  void import('react-grab')
  void import('react-scan')
}

const isFloatingWindow = new URLSearchParams(window.location.search).get('view') === 'transfer-float'
if (isFloatingWindow) {
  document.documentElement.classList.add('floating-window-root')
  document.body.classList.add('floating-window-body')
}

const rootElement = document.getElementById('root')!
const root = createRoot(rootElement)

async function bootstrap() {
  let RootComponent: ComponentType
  if (isFloatingWindow) {
    const mod = await import('./features/transfers/FloatingTransferWindow')
    RootComponent = mod.FloatingTransferWindow
  } else {
    const mod = await import('./App')
    RootComponent = mod.default
  }
  root.render(
    <StrictMode>
      <RootComponent />
    </StrictMode>,
  )
}

void bootstrap()
