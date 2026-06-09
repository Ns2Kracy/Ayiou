export type PluginLifecycleState =
  | 'Registered'
  | 'Initializing'
  | 'Starting'
  | 'Running'
  | 'Stopping'
  | 'Stopped'
  | 'Failed'

export type PluginAction = 'enable' | 'disable' | 'start' | 'stop' | 'reload'

export interface PluginSnapshot {
  instance_id: string
  kind: string
  reloadable: boolean
  manifest: {
    kind: string
    description: string
    version: string
    required_capabilities: string[]
    optional_capabilities: string[]
    required_services: string[]
    optional_services: string[]
  }
  lifecycle: {
    enabled: boolean
    desired_config_version: number
    applied_config_version: number
    config_lifecycle_state: string
    lifecycle_state: PluginLifecycleState
    last_error: string | null
  }
  health: {
    healthy: boolean
    detail: string | null
  }
}

interface ApiSuccess<T> {
  ok: true
  data: T
}

interface ApiFailure {
  ok: false
  error: {
    code: string
    message: string
  }
}

type ApiEnvelope<T> = ApiSuccess<T> | ApiFailure

async function request<T>(path: string, token: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      Authorization: `Bearer ${token}`,
      ...init?.headers,
    },
  })
  const envelope = (await response.json()) as ApiEnvelope<T>

  if (!envelope.ok) {
    throw new Error(`${envelope.error.code}: ${envelope.error.message}`)
  }

  return envelope.data
}

export function listPlugins(token: string): Promise<PluginSnapshot[]> {
  return request<PluginSnapshot[]>('/api/plugins', token)
}

export function pluginAction(token: string, id: string, action: PluginAction): Promise<void> {
  return request(`/api/plugins/${encodeURIComponent(id)}/${action}`, token, {
    method: 'POST',
  }).then(() => undefined)
}
