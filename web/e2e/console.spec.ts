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

import { expect, test } from '@playwright/test'

const ADMIN_TOKEN = process.env.AIHUB_ADMIN_TOKEN ?? ''

test('first launch shows login and authenticates with admin token', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Enterprise AI Hub' })).toBeVisible()
  await page.getByRole('textbox', { name: 'Admin Token' }).fill(ADMIN_TOKEN)
  await page.getByRole('button', { name: '进入控制台' }).click()
  await expect(page.getByRole('heading', { name: '总览' })).toBeVisible()
})

test('providers page lists providers table', async ({ page }) => {
  await page.goto('/')
  await page.getByRole('textbox', { name: 'Admin Token' }).fill(ADMIN_TOKEN)
  await page.getByRole('button', { name: '进入控制台' }).click()
  await page.getByRole('link', { name: 'Providers' }).click()
  await expect(page.getByRole('heading', { name: 'Providers' })).toBeVisible()
})

test('gateway /v1/models requires auth (OpenAI compatibility surface)', async ({ request }) => {
  const response = await request.get('/v1/models')
  expect(response.status()).toBe(401)
})
