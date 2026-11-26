import { Download, Eye, EyeOff, KeyRound, ShieldAlert, ShieldPlus } from 'lucide-react'
import { useId, useState } from 'react'
import { Modal } from '../../components/Modal'
import type { VaultSummary } from '../../types/backend'

export type VaultSafeKeyPrompt =
  | { readonly kind: 'unlock'; readonly vault: VaultSummary }
  | { readonly kind: 'create'; readonly name: string; readonly parentPath: string }
  | { readonly kind: 'export'; readonly vault: VaultSummary }
  | { readonly kind: 'remove'; readonly vault: VaultSummary }

interface VaultSafeKeyModalProps {
  readonly prompt: VaultSafeKeyPrompt
  readonly submitting: boolean
  readonly error: string
  readonly onClose: () => void
  readonly onSubmit: (safeKey: string, confirmation: string) => Promise<void>
}

const REMOVE_CONFIRMATION_TEXT = '移除保险箱'

export function VaultSafeKeyModal({
  prompt,
  submitting,
  error,
  onClose,
  onSubmit,
}: VaultSafeKeyModalProps) {
  const [safeKey, setSafeKey] = useState('')
  const [confirmKey, setConfirmKey] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const [showKey, setShowKey] = useState(false)
  const [showConfirmKey, setShowConfirmKey] = useState(false)
  const [localError, setLocalError] = useState('')
  const formId = useId()
  const keyInputId = useId()
  const confirmKeyInputId = useId()
  const removeConfirmInputId = useId()

  const isCreate = prompt.kind === 'create'
  const isRemove = prompt.kind === 'remove'
  const vaultName = prompt.kind === 'create' ? prompt.name : prompt.vault.name

  const title =
    prompt.kind === 'unlock'
      ? '解锁保险箱'
      : prompt.kind === 'create'
        ? '设置 Safe Key'
        : prompt.kind === 'export'
          ? '导出 rclone 配置'
          : '移除保险箱'

  const subtitle =
    prompt.kind === 'unlock'
      ? '输入此保险箱的 Safe Key 以在本地解密文件，密钥不会上传至服务器。'
      : prompt.kind === 'create'
        ? 'Safe Key 仅用于本地端到端加密，不会上传到服务器；遗忘后无法找回，请妥善保管。'
        : prompt.kind === 'export'
          ? '验证 Safe Key 后，将选择本地位置保存兼容 rclone 的加密配置文件。'
          : '移除后不会删除 Koofr 云端中的加密文件，仅取消该保险箱的关联。'

  const actionLabel =
    prompt.kind === 'unlock'
      ? (submitting ? '正在解锁…' : '解锁')
      : prompt.kind === 'create'
        ? (submitting ? '正在创建…' : '创建并解锁')
        : prompt.kind === 'export'
          ? (submitting ? '正在验证…' : '验证并导出')
          : (submitting ? '正在移除…' : '确认移除')

  const keyTooShort = isCreate && safeKey.length > 0 && Array.from(safeKey).length < 8
  const keysMismatch = isCreate && confirmKey.length > 0 && safeKey !== confirmKey

  const canSubmit = !submitting && (
    prompt.kind === 'create'
      ? Array.from(safeKey).length >= 8 && safeKey === confirmKey
      : prompt.kind === 'remove'
        ? confirmation === REMOVE_CONFIRMATION_TEXT && safeKey.length > 0
        : safeKey.length > 0
  )

  const submit = async () => {
    if (!canSubmit) return
    setLocalError('')
    if (isCreate) {
      if (Array.from(safeKey).length < 8) {
        setLocalError('Safe Key 至少需要 8 个字符。')
        return
      }
      if (safeKey !== confirmKey) {
        setLocalError('两次输入的 Safe Key 不一致。')
        return
      }
    }
    const keyToSend = safeKey
    const confirmToSend = confirmation
    await onSubmit(keyToSend, confirmToSend)
  }

  const displayError = localError || error

  return (
    <Modal
      title={title}
      actionLabel={actionLabel}
      secondaryActionLabel="取消"
      actionDisabled={!canSubmit}
      secondaryActionDisabled={submitting}
      className={isRemove ? 'vault-key-modal vault-key-modal--danger' : 'vault-key-modal'}
      onClose={onClose}
      onSecondaryAction={onClose}
      onAction={() => void submit()}
    >
      <form
        id={formId}
        className="vault-key-form"
        onSubmit={(event) => {
          event.preventDefault()
          void submit()
        }}
      >
        <div className="vault-key-header">
          <span
            className={`vault-key-badge${isRemove ? ' vault-key-badge--danger' : ''}`}
            aria-hidden="true"
          >
            {prompt.kind === 'unlock' ? (
              <KeyRound size={20} strokeWidth={1.85} />
            ) : prompt.kind === 'create' ? (
              <ShieldPlus size={20} strokeWidth={1.85} />
            ) : prompt.kind === 'export' ? (
              <Download size={20} strokeWidth={1.85} />
            ) : (
              <ShieldAlert size={20} strokeWidth={1.85} />
            )}
          </span>
          <div className="vault-key-header__copy">
            <div className="vault-key-target">
              <strong translate="no">{vaultName}</strong>
              {prompt.kind === 'create' ? (
                <span className="vault-key-path" translate="no">{prompt.parentPath}</span>
              ) : null}
            </div>
            <p>{subtitle}</p>
          </div>
        </div>

        {displayError ? (
          <div className="vault-error" role="alert">{displayError}</div>
        ) : null}

        {isRemove ? (
          <div className="vault-key-field">
            <label htmlFor={removeConfirmInputId}>
              确认文本（请输入“{REMOVE_CONFIRMATION_TEXT}”）
            </label>
            <input
              id={removeConfirmInputId}
              autoFocus
              type="text"
              autoComplete="off"
              spellCheck={false}
              disabled={submitting}
              placeholder={REMOVE_CONFIRMATION_TEXT}
              value={confirmation}
              onChange={(event) => setConfirmation(event.target.value)}
            />
          </div>
        ) : null}

        <div className="vault-key-field">
          <label htmlFor={keyInputId}>
            {isCreate ? 'Safe Key（至少 8 个字符）' : 'Safe Key'}
          </label>
          <div className="vault-key-input-wrap">
            <input
              id={keyInputId}
              autoFocus={!isRemove}
              type={showKey ? 'text' : 'password'}
              autoComplete="off"
              spellCheck={false}
              disabled={submitting}
              placeholder={isCreate ? '输入强密码作为 Safe Key' : '输入保险箱 Safe Key'}
              value={safeKey}
              onChange={(event) => {
                setSafeKey(event.target.value)
                if (localError) setLocalError('')
              }}
            />
            <button
              type="button"
              className="vault-key-toggle"
              aria-label={showKey ? '隐藏 Safe Key' : '显示 Safe Key'}
              disabled={submitting}
              onClick={() => setShowKey((visible) => !visible)}
            >
              {showKey ? <EyeOff size={15} strokeWidth={1.8} /> : <Eye size={15} strokeWidth={1.8} />}
            </button>
          </div>
          {keyTooShort ? (
            <span className="vault-key-hint vault-key-hint--warn">还需要至少 {8 - Array.from(safeKey).length} 个字符</span>
          ) : null}
        </div>

        {isCreate ? (
          <div className="vault-key-field">
            <label htmlFor={confirmKeyInputId}>确认 Safe Key</label>
            <div className="vault-key-input-wrap">
              <input
                id={confirmKeyInputId}
                type={showConfirmKey ? 'text' : 'password'}
                autoComplete="off"
                spellCheck={false}
                disabled={submitting}
                placeholder="再次输入 Safe Key 以确认"
                value={confirmKey}
                onChange={(event) => {
                  setConfirmKey(event.target.value)
                  if (localError) setLocalError('')
                }}
              />
              <button
                type="button"
                className="vault-key-toggle"
                aria-label={showConfirmKey ? '隐藏确认 Safe Key' : '显示确认 Safe Key'}
                disabled={submitting}
                onClick={() => setShowConfirmKey((visible) => !visible)}
              >
                {showConfirmKey ? <EyeOff size={15} strokeWidth={1.8} /> : <Eye size={15} strokeWidth={1.8} />}
              </button>
            </div>
            {keysMismatch ? (
              <span className="vault-key-hint vault-key-hint--warn">两次输入的 Safe Key 不一致</span>
            ) : null}
          </div>
        ) : null}

        <button type="submit" hidden disabled={!canSubmit} aria-hidden="true" tabIndex={-1} />
      </form>
    </Modal>
  )
}
