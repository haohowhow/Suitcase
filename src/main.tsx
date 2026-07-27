if (import.meta.env.DEV) {
  void import('react-grab')
  void import('react-scan')
}

import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { FloatingTransferWindow } from './features/transfers/FloatingTransferWindow'
import './styles.css'

const isFloatingWindow = new URLSearchParams(window.location.search).get('view') === 'transfer-float'
if (isFloatingWindow) {
  document.documentElement.classList.add('floating-window-root')
  document.body.classList.add('floating-window-body')
}

const rootComponent = isFloatingWindow
  ? <FloatingTransferWindow />
  : <App />

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {rootComponent}
  </StrictMode>,
)
