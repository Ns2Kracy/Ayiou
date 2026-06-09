import { Button, Card, Chip, Input } from '@heroui/react'
import {
  ArrowClockwise,
  ArrowsClockwise,
  CheckCircle,
  Pause,
  Play,
  Power,
  Prohibit,
  Pulse,
  ShieldCheck,
  Stop,
  XCircle,
} from '@phosphor-icons/react'
import type { ReactNode } from 'react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { listPlugins, pluginAction, type PluginAction, type PluginSnapshot } from './api'

const tokenStorageKey = 'ayiou.control.token'

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

  const runningCount = plugins.filter(
    (plugin) => plugin.lifecycle.lifecycle_state === 'Running',
  ).length
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

  async function runAction(plugin: PluginSnapshot, action: PluginAction) {
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
              Ayiou Control Plane
            </div>
            <h1 className="mt-2 text-2xl font-semibold tracking-normal text-white sm:text-3xl">
              Plugin runtime management
            </h1>
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
          <Metric label="Plugins" value={plugins.length} detail="registered instances" />
          <Metric label="Running" value={runningCount} detail="active lifecycle" />
          <Metric label="Healthy" value={healthyCount} detail="reported healthy" />
          <Metric label="Disabled" value={disabledCount} detail="not dispatching" />
        </section>

        <section className="mb-4 min-h-11 rounded-md border border-zinc-800 bg-zinc-900 px-4 py-3 text-sm">
          <div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
            <span className="text-zinc-300">{result}</span>
            {loading ? <span className="text-emerald-300">Working...</span> : null}
          </div>
          {error ? <div className="mt-2 text-red-300">{error}</div> : null}
        </section>

        <div className="grid min-h-0 flex-1 gap-4 lg:grid-cols-[minmax(0,1.15fr)_minmax(360px,0.85fr)]">
          <section className="min-h-0 overflow-hidden rounded-md border border-zinc-800 bg-zinc-900">
            <div className="flex items-center justify-between border-b border-zinc-800 px-4 py-3">
              <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-400">
                Plugins
              </h2>
              <span className="text-xs text-zinc-500">{plugins.length} total</span>
            </div>
            <div className="overflow-auto">
              <table className="w-full min-w-[760px] border-collapse text-left text-sm">
                <thead className="bg-zinc-950/70 text-xs uppercase tracking-wide text-zinc-500">
                  <tr>
                    <th className="px-4 py-3 font-medium">Instance</th>
                    <th className="px-4 py-3 font-medium">Lifecycle</th>
                    <th className="px-4 py-3 font-medium">Enabled</th>
                    <th className="px-4 py-3 font-medium">Health</th>
                    <th className="px-4 py-3 font-medium">Version</th>
                    <th className="px-4 py-3 font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {plugins.map((plugin) => (
                    <PluginRow
                      key={plugin.instance_id}
                      plugin={plugin}
                      selected={plugin.instance_id === selected?.instance_id}
                      busy={loading}
                      onSelect={() => setSelectedId(plugin.instance_id)}
                      onAction={runAction}
                    />
                  ))}
                </tbody>
              </table>
              {plugins.length === 0 ? (
                <div className="flex min-h-64 items-center justify-center text-sm text-zinc-500">
                  No plugin snapshots loaded.
                </div>
              ) : null}
            </div>
          </section>

          <PluginDetails plugin={selected} />
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

function PluginRow({
  plugin,
  selected,
  busy,
  onSelect,
  onAction,
}: {
  plugin: PluginSnapshot
  selected: boolean
  busy: boolean
  onSelect: () => void
  onAction: (plugin: PluginSnapshot, action: PluginAction) => Promise<void>
}) {
  return (
    <tr className={selected ? 'bg-emerald-400/10' : 'border-t border-zinc-800'}>
      <td className="px-4 py-3 align-top">
        <button
          aria-label={`Select ${plugin.instance_id}`}
          className="text-left"
          type="button"
          onClick={onSelect}
        >
          <div className="font-medium text-zinc-100">{plugin.instance_id}</div>
          <div className="mt-1 text-xs text-zinc-500">{plugin.kind}</div>
        </button>
      </td>
      <td className="px-4 py-3 align-top">
        <StatusPill value={plugin.lifecycle.lifecycle_state} />
      </td>
      <td className="px-4 py-3 align-top">
        {plugin.lifecycle.enabled ? (
          <Chip
            className="border border-emerald-400/30 bg-emerald-400/10 text-emerald-300"
            size="sm"
            variant="soft"
          >
            <CheckCircle className="h-4 w-4" /> enabled
          </Chip>
        ) : (
          <Chip
            className="border border-zinc-700 bg-zinc-950 text-zinc-500"
            size="sm"
            variant="soft"
          >
            <Prohibit className="h-4 w-4" /> disabled
          </Chip>
        )}
      </td>
      <td className="px-4 py-3 align-top">
        {plugin.health.healthy ? (
          <Chip
            className="border border-emerald-400/30 bg-emerald-400/10 text-emerald-300"
            size="sm"
            variant="soft"
          >
            <Pulse className="h-4 w-4" /> healthy
          </Chip>
        ) : (
          <Chip
            className="border border-red-400/30 bg-red-400/10 text-red-300"
            size="sm"
            variant="soft"
          >
            <XCircle className="h-4 w-4" /> unhealthy
          </Chip>
        )}
      </td>
      <td className="px-4 py-3 align-top text-zinc-300">{plugin.manifest.version}</td>
      <td className="px-4 py-3 align-top">
        <div className="flex flex-wrap gap-1.5">
          <IconButton
            aria-label={plugin.lifecycle.enabled ? 'Disable' : 'Enable'}
            disabled={busy}
            onClick={() => void onAction(plugin, plugin.lifecycle.enabled ? 'disable' : 'enable')}
          >
            <Power className="h-4 w-4" />
          </IconButton>
          <IconButton
            aria-label="Start"
            disabled={busy}
            onClick={() => void onAction(plugin, 'start')}
          >
            <Play className="h-4 w-4" />
          </IconButton>
          <IconButton
            aria-label="Stop"
            disabled={busy}
            onClick={() => void onAction(plugin, 'stop')}
          >
            <Stop className="h-4 w-4" />
          </IconButton>
          <IconButton
            aria-label="Reload"
            disabled={busy || !plugin.reloadable}
            onClick={() => void onAction(plugin, 'reload')}
          >
            <ArrowsClockwise className="h-4 w-4" />
          </IconButton>
        </div>
      </td>
    </tr>
  )
}

function IconButton({
  'aria-label': ariaLabel,
  disabled,
  onClick,
  children,
}: {
  'aria-label': string
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      aria-label={ariaLabel}
      className="inline-flex h-8 w-8 min-w-8 items-center justify-center rounded-md border border-zinc-700 bg-zinc-950 text-zinc-300 transition-colors hover:border-emerald-400 hover:text-emerald-300 disabled:cursor-not-allowed disabled:opacity-50"
      disabled={disabled}
      type="button"
      onClick={onClick}
    >
      {children}
    </button>
  )
}

function StatusPill({ value }: { value: string }) {
  const tone =
    value === 'Running'
      ? 'border-emerald-400/40 bg-emerald-400/10 text-emerald-300'
      : value === 'Failed'
        ? 'border-red-400/40 bg-red-400/10 text-red-300'
        : 'border-zinc-700 bg-zinc-950 text-zinc-300'

  return (
    <Chip className={tone} size="sm" variant="soft">
      {value}
    </Chip>
  )
}

function PluginDetails({ plugin }: { plugin?: PluginSnapshot }) {
  if (!plugin) {
    return (
      <aside className="rounded-md border border-zinc-800 bg-zinc-900 p-4 text-sm text-zinc-500">
        Select a plugin to inspect runtime details.
      </aside>
    )
  }

  return (
    <aside className="min-h-0 overflow-auto rounded-md border border-zinc-800 bg-zinc-900">
      <div className="border-b border-zinc-800 px-4 py-3">
        <h2 className="text-lg font-semibold text-white">{plugin.instance_id}</h2>
        <p className="mt-1 text-sm text-zinc-500">
          {plugin.manifest.description || 'No description'}
        </p>
      </div>
      <div className="space-y-4 p-4">
        <DetailGrid
          items={[
            ['Kind', plugin.kind],
            ['Version', plugin.manifest.version],
            ['Config', plugin.lifecycle.config_lifecycle_state],
            ['Desired config', String(plugin.lifecycle.desired_config_version)],
            ['Applied config', String(plugin.lifecycle.applied_config_version)],
            ['Reloadable', plugin.reloadable ? 'yes' : 'no'],
          ]}
        />

        <Section title="Capabilities">
          <TagGroup label="Required" values={plugin.manifest.required_capabilities} />
          <TagGroup label="Optional" values={plugin.manifest.optional_capabilities} />
        </Section>

        <Section title="Services">
          <TagGroup label="Required" values={plugin.manifest.required_services} />
          <TagGroup label="Optional" values={plugin.manifest.optional_services} />
        </Section>

        <Section title="Health">
          <div className="flex items-center gap-2 text-sm text-zinc-300">
            {plugin.health.healthy ? (
              <CheckCircle className="h-4 w-4 text-emerald-300" />
            ) : (
              <Pause className="h-4 w-4 text-red-300" />
            )}
            {plugin.health.healthy ? 'healthy' : 'unhealthy'}
          </div>
          {plugin.health.detail ? (
            <p className="mt-2 text-sm text-zinc-500">{plugin.health.detail}</p>
          ) : null}
        </Section>

        {plugin.lifecycle.last_error ? (
          <Section title="Last error">
            <pre className="overflow-auto rounded-md border border-red-400/30 bg-red-950/30 p-3 text-xs text-red-200">
              {plugin.lifecycle.last_error}
            </pre>
          </Section>
        ) : null}
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
