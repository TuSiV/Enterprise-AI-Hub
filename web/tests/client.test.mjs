// Copyright 2026 YONGZHE CHEN
// SPDX-License-Identifier: Apache-2.0
import { test, beforeEach } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import ts from 'typescript'

let data
let events
let calls
let response
let client
beforeEach(async () => {
  data = new Map([['aihub_admin_token', 'legacy-secret']])
  globalThis.localStorage = {
    getItem: key => data.get(key) ?? null,
    setItem: (key, value) => data.set(key, value),
    removeItem: key => data.delete(key),
  }
  events = []
  calls = []
  globalThis.window = { dispatchEvent: event => events.push(event.type) }
  response = new Response(JSON.stringify({ data: { ok: true } }), { status: 200 })
  globalThis.fetch = async (url, init) => { calls.push({ url, init }); return response }
  const source = readFileSync(new URL('../src/api/client.ts', import.meta.url), 'utf8')
    .replace("import { t } from '../i18n'", 'const t = (text: string) => text')
  const output = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
  }).outputText
  client = await import(`data:text/javascript;base64,${Buffer.from(output).toString('base64')}#${Math.random()}`)
})

test('legacy credentials are removed and new credentials remain in memory', () => {
  assert.equal(data.has('aihub_admin_token'), false)
  assert.equal(client.getToken(), null)
  client.setToken('test-secret')
  assert.equal(client.getToken(), 'test-secret')
  assert.equal(data.has('aihub_admin_token'), false)
  client.clearToken()
  assert.equal(client.getToken(), null)
})

test('JSON and streaming transport use the selected server and credential', async () => {
  client.setServerUrl('https://hub.example/prefix/')
  client.setToken('test-secret')
  assert.deepEqual(await client.api.get('/api/v1/admin/config'), { ok: true })
  response = new Response('data: hello\n\n', { headers: { 'Content-Type': 'text/event-stream' } })
  const stream = await client.authenticatedFetch('/api/v1/admin/playground/stream', { method: 'POST', body: '{}' })
  assert.equal(await stream.text(), 'data: hello\n\n')
  assert.equal(calls[0].url, 'https://hub.example/prefix/api/v1/admin/config')
  assert.equal(calls[1].url, 'https://hub.example/prefix/api/v1/admin/playground/stream')
  assert.equal(calls[1].init.headers.get('Authorization'), 'Bearer test-secret')
})

test('streaming 401 clears credentials and triggers the common unauthorized event', async () => {
  client.setToken('expired')
  response = new Response('{}', { status: 401 })
  await assert.rejects(client.authenticatedFetch('/api/v1/admin/playground/stream'), error => error.status === 401)
  assert.equal(client.getToken(), null)
  assert.deepEqual(events, ['aihub:unauthorized'])
})

test('invalid server URLs cannot receive credentials', () => {
  for (const url of ['javascript:alert(1)', 'https://name:password@hub.example', 'https://hub.example/#x', 'https://hub.example/?x']) {
    assert.throws(() => client.setServerUrl(url), TypeError)
  }
  assert.equal(calls.length, 0)
})
