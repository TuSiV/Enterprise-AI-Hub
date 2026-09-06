import { useSyncExternalStore } from 'react'
import { messages } from './messages'
export type Locale = 'zh' | 'en'
const key = 'aihub_locale'
let locale: Locale = localStorage.getItem(key) === 'en' ? 'en' : 'zh'
const listeners = new Set<() => void>()
function updateDocument() { document.documentElement.lang = locale === 'zh' ? 'zh-CN' : 'en' }
updateDocument()
export function setLocale(value: Locale) {
  locale = value
  localStorage.setItem(key, value)
  updateDocument()
  listeners.forEach(listener => listener())
}
export function useLocale() {
  return useSyncExternalStore(listener => { listeners.add(listener); return () => { listeners.delete(listener) } }, () => locale)
}
export function t(text: string, values: unknown[] = []): string {
  const translated = locale === 'en' ? messages[text] ?? text : text
  return translated.replace(/\{(\d+)\}/g, (_, index) => String(values[Number(index)] ?? ''))
}
export function number(value: number) { return value.toLocaleString(locale === 'zh' ? 'zh-CN' : 'en-US') }
export function LanguageSwitch() {
  const current = useLocale()
  return <select className="language-switch" aria-label="Language / 语言" value={current} onChange={event => setLocale(event.target.value as Locale)}><option value="zh">简体中文</option><option value="en">English</option></select>
}
