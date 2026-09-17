/**
 * Citation popover for the AI answer surface (plan step 19: Base UI parts where behavior
 * demands it). `@base-ui/react`'s Popover parts own focus, keyboard dismissal and portal
 * rendering; daisyUI owns the look via `card`/`link` class names, composed with the `render`
 * prop per the ui/AGENTS.md doctrine.
 *
 * Each citation marker `[n]` in the answer opens the matching source's title + url; the
 * popover trigger is a small badge button, the popup links out to the source in a new tab.
 */
import { Popover as BasePopover } from '@base-ui/react/popover'
import { Trans } from '@lingui/react/macro'
import type { components } from '../lib/types.gen.ts'

type AnswerSource = components['schemas']['AnswerSource']

export interface CitationPopoverProps {
  index: number
  source: AnswerSource
}

export function CitationPopover({ index, source }: CitationPopoverProps) {
  return (
    <BasePopover.Root>
      <BasePopover.Trigger
        render={<button type="button" className="btn btn-ghost btn-xs align-baseline px-1" />}
        aria-label={`source ${String(index)}`}
      >
        [{index}]
      </BasePopover.Trigger>
      <BasePopover.Portal>
        <BasePopover.Positioner sideOffset={6}>
          <BasePopover.Popup
            className="card bg-base-100 shadow-xl border border-base-300 p-3 text-sm max-w-xs"
            render={<div className="card bg-base-100 shadow-xl border border-base-300 p-3 text-sm max-w-xs" />}
          >
            <BasePopover.Title className="font-semibold mb-1">{source.title}</BasePopover.Title>
            <a href={source.url} target="_blank" rel="noreferrer" className="link link-primary break-all">
              <Trans comment="open citation source">open source</Trans>
            </a>
          </BasePopover.Popup>
        </BasePopover.Positioner>
      </BasePopover.Portal>
    </BasePopover.Root>
  )
}
