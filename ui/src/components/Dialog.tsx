/**
 * Minimal reusable dialog primitive proving the plan's one-sentence doctrine
 * (ui/AGENTS.md, "The daisyUI / Base UI doctrine"): "if it is looks, it is a daisyUI class;
 * if it is focus, keyboard or portal, it is a Base UI part. They compose on the same element
 * via Base UI's `render` prop."
 *
 * `@base-ui/react`'s `Dialog.*` parts own focus trapping, keyboard (Escape to close, focus
 * restore on unmount) and portal rendering -- none of that is hand-rolled here. daisyUI's
 * `modal-box` / `modal-backdrop` / `btn` class names own the look. The `render` prop is what
 * lets a Base UI part (which owns behavior/ARIA wiring) emit a plain host element carrying
 * daisyUI classes, instead of Base UI's default unstyled element.
 *
 * Nothing calls this yet -- it's the foundation-task proof that the pattern compiles and
 * composes, so screen tasks (settings dialog, etc.) reuse it instead of each reinventing the
 * Base UI + daisyUI wiring independently.
 */
import { Trans } from '@lingui/react/macro'
import { Dialog as BaseDialog } from '@base-ui/react/dialog'
import type { ReactNode } from 'react'

export interface DialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: ReactNode
  children: ReactNode
}

export function Dialog({ open, onOpenChange, title, children }: DialogProps) {
  return (
    <BaseDialog.Root open={open} onOpenChange={onOpenChange}>
      <BaseDialog.Portal>
        {/* Base UI part (backdrop presence/animation state) rendered as a daisyUI-styled
         * host element via `render`, instead of Base UI's default unstyled <div>. */}
        <BaseDialog.Backdrop render={<div className="modal-backdrop bg-black/50" />} />
        {/* daisyUI 5's `.modal-box` ships `opacity: 0` and only becomes visible under a
         * `.modal`/`.modal-open` ancestor, so the popup is rendered as a daisyUI
         * `<dialog className="modal modal-open">` wrapping the modal-box (the same
         * markup daisyUI's own open-modal example produces). Base UI still owns the
         * ARIA/behavior via the Popup part; daisyUI owns the look. */}
        <BaseDialog.Popup render={<dialog className="modal modal-open" />}>
          <div className="modal-box">
            <BaseDialog.Title className="text-lg font-bold">{title}</BaseDialog.Title>
            <div className="py-4">{children}</div>
            <div className="modal-action">
              {/* daisyUI's `btn` look on Base UI's Close part (keyboard + click dismissal,
             * focus restore) via `render`. */}
              <BaseDialog.Close render={<button type="button" className="btn" />}>
                <Trans>Close</Trans>
              </BaseDialog.Close>
            </div>
          </div>
        </BaseDialog.Popup>
      </BaseDialog.Portal>
    </BaseDialog.Root>
  )
}
