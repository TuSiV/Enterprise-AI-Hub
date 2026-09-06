// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
