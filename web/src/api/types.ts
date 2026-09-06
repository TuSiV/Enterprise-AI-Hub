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

// Admin API 类型（与 crates/api-types/src/admin.rs 的 camelCase DTO 一一对应）

export interface ProviderDto {
  id: string
  key: string
  name: string
  kind: string
  baseUrl: string
  credentialConfigured: boolean
  timeoutMs: number
  maxRetries: number
  enabled: boolean
  status: string
  health: string
  lastHealthCheckAt: string | null
  modelCount: number
  config: any
  createdAt: string
  updatedAt: string
}

export interface ModelDto {
  id: string
  providerId: string
  providerKey: string | null
  providerName: string | null
  modelKey: string
  displayName: string
  modelType: string
  contextWindow: number | null
  maxOutputTokens: number | null
  capabilities: any
  pricing: Pricing
  enabled: boolean
  discovered: boolean
  metadata: any
  createdAt: string
  updatedAt: string
}

export interface Pricing {
  currency?: string
  unitTokens?: number
  input?: number | null
  output?: number | null
  cachedInput?: number | null
  reasoning?: number | null
}

export interface VirtualModelTargetDto {
  id: string
  modelId: string
  modelLabel: string | null
  priority: number
  weight: number
  enabled: boolean
  condition: any
  overrides: any
}

export interface VirtualModelDto {
  id: string
  key: string
  name: string
  description: string | null
  routingStrategy: string
  enabled: boolean
  config: any
  targets: VirtualModelTargetDto[]
  createdAt: string
  updatedAt: string
}

export interface QuotaInput {
  rpm?: number | null
  tpm?: number | null
  dailyRequests?: number | null
  monthlyTokens?: number | null
  monthlyCostMicrounits?: number | null
  exceedAction?: string
}

export interface QuotaPolicyDto {
  rpm: number | null
  tpm: number | null
  dailyRequests: number | null
  monthlyTokens: number | null
  monthlyCostMicrounits: number | null
  exceedAction: string
}

export interface ApplicationDto {
  id: string
  key: string
  name: string
  status: string
  allowedVirtualModels: string[]
  allowDirectModels: boolean
  monthlyBudgetMicrounits: number | null
  quota: QuotaPolicyDto | null
  keyCount: number
  metadata: any
  createdAt: string
  updatedAt: string
}

export interface ApiKeyDto {
  id: string
  applicationId: string
  name: string
  prefix: string
  maskedKey: string
  scopes: string[]
  expiresAt: string | null
  lastUsedAt: string | null
  revokedAt: string | null
  createdAt: string
}

export interface UsageSummary {
  requests: number
  successRequests: number
  failedRequests: number
  inputTokens: number
  outputTokens: number
  totalTokens: number
  costMicrounits: number
  currency: string
  successRate: number
  p50LatencyMs: number | null
  p95LatencyMs: number | null
  avgTtftMs: number | null
  cacheHitRate: number
}

export interface TimeseriesPoint {
  bucket: string
  requests: number
  inputTokens: number
  outputTokens: number
  costMicrounits: number
  errors: number
}

export interface GroupUsage {
  group: string
  requests: number
  totalTokens: number
  costMicrounits: number
}

export interface RequestListItem {
  id: string
  traceId: string | null
  applicationId: string | null
  applicationKey: string | null
  endpoint: string
  requestedModel: string
  resolvedModelKey: string | null
  providerKey: string | null
  status: string
  httpStatus: number | null
  startedAt: string
  latencyMs: number | null
  ttftMs: number | null
  retryCount: number
  totalTokens: number | null
  costMicrounits: number | null
  errorCode: string | null
}

export interface RequestDetail extends RequestListItem {
  apiKeyId: string | null
  completedAt: string | null
  cacheStatus: string | null
  errorMessage: string | null
  usage: any
  cost: any
  metadata: any
}

export interface AuditEventDto {
  id: string
  traceId: string | null
  actorType: string
  actorId: string | null
  eventType: string
  resourceType: string | null
  resourceId: string | null
  decision: string | null
  metadata: any
  createdAt: string
}

export interface SystemInfo {
  version: string
  mode: string
  gatewayEndpoint: string
  dbDriver: string
  startedAt: string
}

export interface RouteSimulation {
  virtualModel: string
  candidates: {
    modelId: string
    label: string
    priority: number
    enabled: boolean
    providerEnabled: boolean
    selected: boolean
    excludedReason: string | null
  }[]
  selected: string | null
  fallbackOrder: string[]
}

export const MICROUPNITS = 1_000_000

export function formatCost(microunits: number | null | undefined, currency = 'USD'): string {
  if (microunits == null) return '-'
  const value = microunits / MICROUPNITS
  return `${currency} ${value.toFixed(value < 1 && value > 0 ? 6 : 2)}`
}

export function formatTokens(n: number | null | undefined): string {
  if (n == null) return '-'
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

export function formatTime(iso: string | null | undefined): string {
  if (!iso) return '-'
  return new Date(iso).toLocaleString(document.documentElement.lang)
}
