/**
 * Settings dialog (userflow-checkpoints.md checkpoints 18/19, screens/settings.md): opened
 * by `?settings=open` on any route (the param is in every route's `validateSearch`), closed
 * or saved by stripping the param back off the url. Look is daisyUI (`modal`, form classes),
 * behavior (portal, focus trap, Escape) is Base UI via the shared `Dialog` primitive --
 * nothing hand-rolled here. Form state is uncontrolled inputs read via FormData (state
 * ladder rung 2); the only async state is the save attempt's outcome.
 */
import { Trans, useLingui } from '@lingui/react/macro'
import { useState } from 'react'
import { Dialog } from '../components/Dialog.tsx'
import { putSettings, type AIConfigPayload } from '../lib/settingsApi.ts'

export interface SettingsDialogProps {
  open: boolean
  onClose: () => void
}

/** Parse the dialog's FormData into the `AIConfig`-shaped PUT payload. */
export function settingsFormPayload(form: FormData): AIConfigPayload {
  const str = (name: string): string => String(form.get(name) ?? '').trim()
  return {
    provider: str('provider'),
    model: str('model'),
    api_key: str('api_key') || null,
    api_key_env: str('api_key_env') || null,
    base_url: str('base_url') || null,
    enabled: form.get('enabled') !== null,
  }
}

export function SettingsDialog({ open, onClose }: SettingsDialogProps) {
  const { t } = useLingui()
  const [saving, setSaving] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  const handleSave = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const payload = settingsFormPayload(new FormData(event.currentTarget))
    setSaving(true)
    setSaveError(null)
    try {
      await putSettings(payload)
      onClose()
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : t`Saving settings failed`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose() }} title={<Trans>Settings</Trans>}>
      <form onSubmit={(event) => { void handleSave(event) }}>
        <fieldset className="fieldset bg-base-200 border border-base-300 rounded-box p-4">
          <legend className="fieldset-legend">
            <Trans>AI</Trans>
          </legend>

          <label className="label" htmlFor="settings-provider">
            <Trans>Provider</Trans>
          </label>
          <select
            id="settings-provider"
            name="provider"
            className="select select-bordered select-sm w-full"
            defaultValue="openai"
          >
            {['openai', 'anthropic', 'groq', 'mistral', 'ollama', 'huggingface'].map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>

          <label className="label" htmlFor="settings-model">
            <Trans>Model</Trans>
          </label>
          <input
            id="settings-model"
            name="model"
            type="text"
            className="input input-bordered input-sm w-full"
            placeholder={t`gpt-4o-mini`}
          />

          <label className="label" htmlFor="settings-api-key">
            <Trans>API key</Trans>
          </label>
          <input
            id="settings-api-key"
            name="api_key"
            type="password"
            className="input input-bordered input-sm w-full"
            autoComplete="off"
          />

          <label className="label" htmlFor="settings-api-key-env">
            <Trans>API key env var</Trans>
          </label>
          <input
            id="settings-api-key-env"
            name="api_key_env"
            type="text"
            className="input input-bordered input-sm w-full"
            placeholder={t`OPENAI_API_KEY`}
          />

          <label className="label" htmlFor="settings-base-url">
            <Trans>Base URL</Trans>
          </label>
          <input
            id="settings-base-url"
            name="base_url"
            type="url"
            className="input input-bordered input-sm w-full"
            placeholder={t`https://api.openai.com/v1`}
          />

          <label className="label cursor-pointer" htmlFor="settings-enabled">
            <span>
              <Trans>AI mode enabled</Trans>
            </span>
            <input
              id="settings-enabled"
              name="enabled"
              type="checkbox"
              className="toggle toggle-sm"
              defaultChecked
            />
          </label>
        </fieldset>

        {saveError !== null && (
          <div role="alert" className="alert alert-error mt-4 text-sm" data-testid="settings-error">
            <span>{saveError}</span>
          </div>
        )}

        <div className="modal-action">
          <button type="submit" className={`btn btn-primary ${saving ? 'btn-disabled' : ''}`}>
            <Trans>Save</Trans>
          </button>
        </div>
      </form>
    </Dialog>
  )
}
