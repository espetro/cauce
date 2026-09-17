import { i18n } from '@lingui/core'
import { messages as enMessages } from '../locales/en/messages'

i18n.load('en', enMessages)
i18n.activate('en')

export { i18n }
