import { memo, useEffect, useState, type FormEvent } from 'react'
import {
  Eye,
  EyeOff,
  LockKeyhole,
  Palette,
  ShieldCheck,
} from 'lucide-react'
import { BrandMark } from '../../components/BrandMark'

interface LoginPageProps {
  busy: boolean
  error: string
  initialEmail: string
  onLogin: (email: string, appPassword: string, rememberPassword: boolean) => Promise<void>
  onThemeClick: () => void
}

export const LoginPage = memo(function LoginPage({ busy, error, initialEmail, onLogin, onThemeClick }: LoginPageProps) {
  const [email, setEmail] = useState(initialEmail)
  const [appPassword, setAppPassword] = useState('')
  const [rememberPassword, setRememberPassword] = useState(Boolean(initialEmail))
  const [showPassword, setShowPassword] = useState(false)
  const [validationError, setValidationError] = useState('')

  useEffect(() => {
    if (initialEmail) {
      setEmail((current) => current || initialEmail)
      setRememberPassword(true)
    }
  }, [initialEmail])

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const normalizedEmail = email.trim()

    if (!normalizedEmail || !normalizedEmail.includes('@')) {
      setValidationError('请输入有效的邮箱地址。')
      return
    }
    if (!appPassword) {
      setValidationError('请输入应用专用密码。')
      return
    }

    setValidationError('')
    await onLogin(normalizedEmail, appPassword, rememberPassword)
    setAppPassword('')
  }

  const visibleError = validationError || error

  return (
    <main className="auth-layout">
      <div className="auth-topbar">
        <button className="auth-theme" type="button" onClick={onThemeClick}>
          <Palette size={15} strokeWidth={1.75} aria-hidden="true" />
          外观主题
        </button>
      </div>

      <section className="auth-panel">
        <form className="auth-form" autoComplete="off" noValidate onSubmit={submit}>
          <header className="auth-form__heading">
            <BrandMark />
            <h1>登录 Koofr</h1>
            <p>使用邮箱和应用专用密码连接你的云端文件。</p>
          </header>

          <div className="auth-field">
            <label htmlFor="koofr-email">邮箱地址</label>
            <input
              id="koofr-email"
              type="email"
              name="koofr-email"
              value={email}
              placeholder="name@example.com"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              disabled={busy}
              aria-invalid={Boolean(visibleError)}
              onChange={(event) => {
                setEmail(event.target.value)
                setValidationError('')
              }}
            />
          </div>

          <div className="auth-field">
            <label htmlFor="koofr-app-password">应用专用密码</label>
            <span className="auth-password">
              <input
                id="koofr-app-password"
                type={showPassword ? 'text' : 'password'}
                name="koofr-app-password"
                value={appPassword}
                placeholder="输入应用专用密码"
                disabled={busy}
                aria-invalid={Boolean(visibleError)}
                onChange={(event) => {
                  setAppPassword(event.target.value)
                  setValidationError('')
                }}
              />
              <button
                type="button"
                aria-label={showPassword ? '隐藏密码' : '显示密码'}
                aria-pressed={showPassword}
                disabled={busy}
                onClick={() => setShowPassword((visible) => !visible)}
              >
                {showPassword ? <EyeOff size={16} strokeWidth={1.75} /> : <Eye size={16} strokeWidth={1.75} />}
              </button>
            </span>
          </div>

          <label className="auth-remember">
            <input
              type="checkbox"
              checked={rememberPassword}
              disabled={busy}
              onChange={(event) => setRememberPassword(event.target.checked)}
            />
            <span>
              <strong>保存密码</strong>
              <small>下次启动时自动连接 Koofr</small>
            </span>
          </label>

          {visibleError ? (
            <div className="auth-error auth-error--visible" role="alert">
              {visibleError}
            </div>
          ) : null}

          <button className="auth-submit" type="submit" disabled={busy}>
            {busy ? <><span className="auth-spinner" />正在连接…</> : '登录'}
          </button>

          <footer className="auth-footnotes" aria-label="Koofr 安全连接说明">
            <p className="auth-hint">
              <LockKeyhole size={14} strokeWidth={1.75} aria-hidden="true" />
              <span>请使用 Koofr 设置中生成的应用专用密码。</span>
            </p>
            <p className="auth-hint">
              <ShieldCheck size={14} strokeWidth={1.75} aria-hidden="true" />
              <span>密码不会写入网页存储；选择记住密码后，将由 Windows 凭据管理器保护。</span>
            </p>
          </footer>
        </form>
      </section>
    </main>
  )
})
