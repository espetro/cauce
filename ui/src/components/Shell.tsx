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
  /** Landing hero: pill centre at 43.5% of viewport height; offset = above-pill stack (9.25rem) + header (4.06rem, 6.4rem when wrapped below sm). */
  hero?: boolean
}

export function Shell({ size = 'lg', align = 'center', hero = false, className = '', children, ...rest }: ShellProps) {
  if (hero) {
    const classes = ['flex min-h-[calc(100vh-4rem)] flex-col items-center justify-start px-4 pt-[max(2rem,calc(43.5vh-15.7rem))] sm:pt-[max(2rem,calc(43.5vh-13.3rem))]', className]
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
