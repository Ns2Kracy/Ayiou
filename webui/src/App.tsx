import { Button, Card, Chip, Input } from '@heroui/react'
import {
  ArrowClockwise,
  CheckCircle,
  Pause,
  PuzzlePiece,
  ShieldCheck,
  WarningCircle,
} from '@phosphor-icons/react'
import type { ReactNode } from 'react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { listPlugins, pluginAction, type PluginAction, type PluginSnapshot } from './api'

const tokenStorageKey = 'ayiou.control.token'

type UserPluginStatusKind = 'running' | 'disabled' | 'failed' | 'starting' | 'stopped' | 'pending'

type UserPluginStatus = {
  kind: UserPluginStatusKind
  label: string
  detail: string
  tone: string
}

type UserPluginAction = Extract<PluginAction, 'enable' | 'disable' | 'reload'>

function getPluginStatus(plugin: PluginSnapshot): UserPluginStatus {
  if (plugin.lifecycle.lifecycle_state === 'Failed' || !plugin.health.healthy) {
    return {
      kind: 'failed',
      label: '异常',
      detail: plugin.lifecycle.last_error ?? plugin.health.detail ?? '插件报告异常状态',
      tone: 'border-red-400/40 bg-red-400/10 text-red-200',
    }
  }

  if (!plugin.lifecycle.enabled) {
    return {
      kind: 'disabled',
      label: '未启用',
      detail: '插件已安装，但不会处理消息。',
      tone: 'border-zinc-700 bg-zinc-900 text-zinc-400',
    }
  }

  if (plugin.lifecycle.lifecycle_state === 'Running') {
    return {
      kind: 'running',
      label: '运行中',
      detail: '插件正在处理匹配的消息和事件。',
      tone: 'border-emerald-400/40 bg-emerald-400/10 text-emerald-200',
    }
  }

  if (
    plugin.lifecycle.lifecycle_state === 'Starting' ||
    plugin.lifecycle.lifecycle_state === 'Initializing'
  ) {
    return {
      kind: 'starting',
      label: '启动中',
      detail: '插件正在初始化，请稍后刷新状态。',
      tone: 'border-amber-400/40 bg-amber-400/10 text-amber-200',
    }
  }

  if (
    plugin.lifecycle.lifecycle_state === 'Stopping' ||
    plugin.lifecycle.lifecycle_state === 'Stopped'
  ) {
    return {
      kind: 'stopped',
      label: '已停止',
      detail: '插件当前没有运行。',
      tone: 'border-zinc-700 bg-zinc-900 text-zinc-300',
    }
  }

  return {
    kind: 'pending',
    label: '待处理',
    detail: '插件已注册，等待运行时更新状态。',
    tone: 'border-sky-400/40 bg-sky-400/10 text-sky-200',
  }
}

function pluginSubtitle(plugin: PluginSnapshot): string {
  return plugin.manifest.description || plugin.kind
}

function App() {
  const [token, setToken] = useState(() => localStorage.getItem(tokenStorageKey) ?? '')
  const [plugins, setPlugins] = useState<PluginSnapshot[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<string>('Ready')
  const [error, setError] = useState<string | null>(null)

  const selected = useMemo(
    () => plugins.find((plugin) => plugin.instance_id === selectedId) ?? plugins[0],
    [plugins, selectedId],
  )

  const runningCount = plugins.filter((plugin) => getPluginStatus(plugin).kind === 'running').length
  const healthyCount = plugins.filter((plugin) => plugin.health.healthy).length
  const disabledCount = plugins.filter((plugin) => !plugin.lifecycle.enabled).length

  const refresh = useCallback(async () => {
    if (!token.trim()) {
      setError('Token is required')
      setResult('Authentication missing')
      return
    }

    setLoading(true)
    setError(null)
    try {
      localStorage.setItem(tokenStorageKey, token)
      const nextPlugins = await listPlugins(token)
      setPlugins(nextPlugins)
      setSelectedId((current) => current ?? nextPlugins[0]?.instance_id ?? null)
      setResult(`Loaded ${nextPlugins.length} plugins`)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setResult('Refresh failed')
    } finally {
      setLoading(false)
    }
  }, [token])

  useEffect(() => {
    if (token) {
      void refresh()
    }
  }, [refresh, token])

  async function runAction(plugin: PluginSnapshot, action: UserPluginAction) {
    setLoading(true)
    setError(null)
    try {
      await pluginAction(token, plugin.instance_id, action)
      setResult(`${action} completed for ${plugin.instance_id}`)
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setResult(`${action} failed for ${plugin.instance_id}`)
    } finally {
      setLoading(false)
    }
  }

  return (
    <main className="min-h-screen bg-zinc-950 text-zinc-100">
      <div className="mx-auto flex min-h-screen w-full max-w-7xl flex-col px-4 py-4 sm:px-6 lg:px-8">
        <header className="flex flex-col gap-4 border-b border-zinc-800 pb-4 lg:flex-row lg:items-end lg:justify-between">
          <div>
            <div className="flex items-center gap-2 text-sm font-medium text-emerald-300">
              <ShieldCheck className="h-4 w-4" />
              Ayiou Console
            </div>
            <h1 className="mt-2 text-2xl font-semibold tracking-normal text-white sm:text-3xl">
              插件管理
            </h1>
            <p className="mt-2 max-w-2xl text-sm text-zinc-400">
              查看插件运行状态，启用或停用插件，并在插件支持时执行热重载。
            </p>
          </div>

          <div className="flex w-full flex-col gap-2 sm:flex-row lg:w-auto">
            <Input
              aria-label="Bearer token"
              className="border-zinc-700 bg-zinc-900 text-zinc-100 sm:w-80"
              type="password"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder="Bearer token"
            />
            <Button
              className="h-10 bg-emerald-400 px-4 font-semibold text-zinc-950"
              type="button"
              onClick={() => void refresh()}
              isDisabled={loading}
            >
              <ArrowClockwise className="h-4 w-4" />
              Refresh
            </Button>
          </div>
        </header>

        <section className="grid gap-3 py-4 sm:grid-cols-2 lg:grid-cols-4">
          <Metric label="插件" value={plugins.length} detail="已加载实例" />
          <Metric label="运行中" value={runningCount} detail="正在处理事件" />
          <Metric label="健康" value={healthyCount} detail="状态正常" />
          <Metric label="未启用" value={disabledCount} detail="不会处理消息" />
        </section>

        <section className="mb-4 min-h-11 rounded-md border border-zinc-800 bg-zinc-900 px-4 py-3 text-sm">
          <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
            <span className="text-zinc-300">{result}</span>
            {loading ? <span className="text-emerald-300">Working...</span> : null}
          </div>
          {error ? <div className="mt-2 text-red-300">{error}</div> : null}
        </section>

        <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[minmax(0,0.9fr)_minmax(360px,1.1fr)]">
          <section className="min-h-0 overflow-hidden rounded-md border border-zinc-800 bg-zinc-900">
            <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
              <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-400">插件</h2>
              <span className="text-xs text-zinc-500">{plugins.length} 个实例</span>
            </div>
            {plugins.length > 0 ? (
              <div className="max-h-full space-y-2 overflow-auto p-3">
                {plugins.map((plugin) => (
                  <PluginListItem
                    key={plugin.instance_id}
                    plugin={plugin}
                    selected={plugin.instance_id === selected?.instance_id}
                    onSelect={() => setSelectedId(plugin.instance_id)}
                  />
                ))}
              </div>
            ) : (
              <div className="flex min-h-80 flex-col items-center justify-center px-6 text-center">
                <div className="flex h-12 w-12 items-center justify-center rounded-full border border-zinc-800 bg-zinc-950 text-zinc-500">
                  <PuzzlePiece className="h-6 w-6" />
                </div>
                <p className="mt-4 text-sm font-medium text-zinc-300">还没有加载插件</p>
                <p className="mt-2 max-w-xs text-sm text-zinc-500">
                  输入 token 后刷新，Console 会显示当前运行时的插件。
                </p>
              </div>
            )}
          </section>

          <PluginDetails plugin={selected} busy={loading} onAction={runAction} />
        </div>
      </div>
    </main>
  )
}

function Metric({ label, value, detail }: { label: string; value: number; detail: string }) {
  return (
    <Card className="border border-zinc-800 bg-zinc-900">
      <Card.Content className="p-4">
        <div className="text-xs font-medium uppercase tracking-wide text-zinc-500">{label}</div>
        <div className="mt-2 text-2xl font-semibold text-white">{value}</div>
        <div className="mt-1 text-xs text-zinc-500">{detail}</div>
      </Card.Content>
    </Card>
  )
}

function PluginListItem({
  plugin,
  selected,
  onSelect,
}: {
  plugin: PluginSnapshot
  selected: boolean
  onSelect: () => void
}) {
  const status = getPluginStatus(plugin)

  return (
    <button
      aria-current={selected ? 'true' : undefined}
      className={`flex w-full items-start justify-between gap-3 rounded-xl border p-4 text-left transition-colors ${
        selected
          ? 'border-emerald-400/50 bg-emerald-400/10 text-emerald-50 shadow-[0_0_0_1px_rgba(52,211,153,0.12)]'
          : 'border-zinc-800 bg-zinc-950/40 text-zinc-100 hover:border-zinc-700 hover:bg-zinc-900/70'
      }`}
      type="button"
      onClick={onSelect}
    >
      <span className="min-w-0">
        <span className="block truncate font-medium">{plugin.instance_id}</span>
        <span className="mt-1 block line-clamp-2 text-xs text-zinc-500">
          {pluginSubtitle(plugin)}
        </span>
      </span>
      <StatusPill status={status} />
    </button>
  )
}
function StatusPill({ status }: { status: UserPluginStatus }) {
  return (
    <Chip className={`shrink-0 border ${status.tone}`} size="sm" variant="soft">
      {status.label}
    </Chip>
  )
}

function PluginDetails({
  plugin,
  busy,
  onAction,
}: {
  plugin?: PluginSnapshot
  busy: boolean
  onAction: (plugin: PluginSnapshot, action: UserPluginAction) => Promise<void>
}) {
  if (!plugin) {
    return (
      <aside className="rounded-2xl border border-zinc-800 bg-zinc-900 p-6 text-sm">
        <h2 className="text-lg font-semibold text-white">选择一个插件</h2>
        <p className="mt-2 max-w-md text-zinc-500">
          从左侧列表选择插件，查看运行状态、健康信息，并按需启用或停用插件。
        </p>
      </aside>
    )
  }

  const status = getPluginStatus(plugin)
  const primaryAction: UserPluginAction = plugin.lifecycle.enabled ? 'disable' : 'enable'
  const primaryLabel = plugin.lifecycle.enabled
    ? '停用插件'
    : status.kind === 'failed'
      ? '重新启用'
      : '启用插件'
  const isUnhealthy = !plugin.health.healthy
  const errorDetail =
    plugin.lifecycle.last_error ?? (isUnhealthy ? plugin.health.detail : undefined)

  return (
    <aside className="min-h-0 overflow-auto rounded-md border border-zinc-800 bg-zinc-900">
      <div className="border-b border-zinc-800 px-4 py-4">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="break-words text-lg font-semibold text-white">{plugin.instance_id}</h2>
              <StatusPill status={status} />
            </div>
            <p className="mt-2 text-sm text-zinc-500">
              {plugin.manifest.description || '这个插件没有提供说明。'}
            </p>
          </div>
          <div className="flex shrink-0 flex-col gap-2 sm:flex-row">
            <Button
              className="bg-emerald-400 font-semibold text-zinc-950"
              type="button"
              onClick={() => void onAction(plugin, primaryAction)}
              isDisabled={busy}
            >
              {primaryLabel}
            </Button>
            {plugin.reloadable ? (
              <Button
                className="border-zinc-700 bg-zinc-950 text-zinc-100"
                type="button"
                variant="outline"
                onClick={() => void onAction(plugin, 'reload')}
                isDisabled={busy}
              >
                热重载
              </Button>
            ) : null}
          </div>
        </div>
      </div>

      <div className="space-y-5 p-4">
        <Section title="运行状态">
          <div className="rounded-xl border border-zinc-800 bg-zinc-950 p-4">
            <div className="flex items-center gap-2 text-sm font-medium text-zinc-200">
              {status.kind === 'failed' ? (
                <WarningCircle className="h-4 w-4 text-red-300" />
              ) : plugin.health.healthy ? (
                <CheckCircle className="h-4 w-4 text-emerald-300" />
              ) : (
                <Pause className="h-4 w-4 text-amber-300" />
              )}
              {status.label}
            </div>
            <p className="mt-2 text-sm text-zinc-500">{status.detail}</p>
            <p className="mt-3 text-sm text-zinc-400">
              健康检查：{plugin.health.healthy ? '状态正常' : '需要处理'}
            </p>
            {plugin.health.detail ? (
              <p className="mt-1 text-sm text-zinc-500">{plugin.health.detail}</p>
            ) : null}
          </div>
        </Section>

        {errorDetail ? (
          <Section title="最近错误">
            <pre className="overflow-auto rounded-md border border-red-400/30 bg-red-950/30 p-3 text-xs leading-5 text-red-200">
              {errorDetail}
            </pre>
          </Section>
        ) : null}

        <Section title="插件信息">
          <DetailGrid
            items={[
              ['版本', plugin.manifest.version],
              ['类型', plugin.kind],
              ['热重载', plugin.reloadable ? '支持' : '不支持'],
              ['配置状态', plugin.lifecycle.config_lifecycle_state],
            ]}
          />
        </Section>

        <Section title="高级信息">
          <DetailGrid
            items={[
              ['Runtime state', plugin.lifecycle.lifecycle_state],
              ['Desired config', String(plugin.lifecycle.desired_config_version)],
              ['Applied config', String(plugin.lifecycle.applied_config_version)],
            ]}
          />
          <TagGroup label="Required capabilities" values={plugin.manifest.required_capabilities} />
          <TagGroup label="Optional capabilities" values={plugin.manifest.optional_capabilities} />
          <TagGroup label="Required services" values={plugin.manifest.required_services} />
          <TagGroup label="Optional services" values={plugin.manifest.optional_services} />
        </Section>
      </div>
    </aside>
  )
}

function DetailGrid({ items }: { items: Array<[string, string]> }) {
  return (
    <dl className="grid grid-cols-2 gap-3 text-sm">
      {items.map(([label, value]) => (
        <div key={label} className="rounded-md border border-zinc-800 bg-zinc-950 p-3">
          <dt className="text-xs uppercase tracking-wide text-zinc-500">{label}</dt>
          <dd className="mt-1 break-words text-zinc-200">{value}</dd>
        </div>
      ))}
    </dl>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">{title}</h3>
      <div className="space-y-2">{children}</div>
    </section>
  )
}

function TagGroup({ label, values }: { label: string; values: string[] }) {
  return (
    <div>
      <div className="mb-1 text-xs text-zinc-500">{label}</div>
      <div className="flex flex-wrap gap-1.5">
        {values.length > 0 ? (
          values.map((value) => (
            <Chip
              key={value}
              className="max-w-full border border-zinc-700 bg-zinc-950 text-zinc-300"
              size="sm"
              variant="soft"
            >
              {value}
            </Chip>
          ))
        ) : (
          <span className="text-xs text-zinc-600">none</span>
        )}
      </div>
    </div>
  )
}

export default App
