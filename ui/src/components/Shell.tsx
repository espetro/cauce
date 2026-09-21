import type { ComponentPropsWithoutRef } from 'react'

type ShellSize = 'sm' | 'md' | 'lg' | 'xl'
type ShellAlign = 'left' | 'center'

const SIZE_CLASS: Record<ShellSize, string> = {
  sm: 'max-w-2xl', // classic SERP list
  md: 'max-w-3xl', // AI answer column
  lg: 'max-w-5xl', // history table
  xl: 'max-w-6xl', // dashboard panel grid
}

interface ShellProps extends ComponentPropsWithoutRef<'main'> {
  size?: ShellSize
  align?: ShellAlign
  /** Landing hero: pill block lands near 43% of the viewport height (patterns-layout-grid.md). */
  hero?: boolean
}

export function Shell({ size = 'lg', align = 'center', hero = false, className = '', children, ...rest }: ShellProps) {
  if (hero) {
    const classes = ['flex min-h-[calc(100vh-4rem)] flex-col items-center justify-start px-4 pt-[max(2rem,calc(43vh-14.6rem))]', className]
      .filter(Boolean)
      .join(' ')
    return (
      <main className={classes} {...rest}>
        {children}
      </main>
    )
  }

  const classes = [align === 'center' ? 'mx-auto' : '', SIZE_CLASS[size], 'px-4 py-8', className]
    .filter(Boolean)
    .join(' ')
  return (
    <main className={classes} {...rest}>
      {children}
    </main>
  )
}
