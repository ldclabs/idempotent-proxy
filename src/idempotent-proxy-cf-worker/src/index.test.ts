import { unstable_dev } from 'wrangler'
import type { UnstableDevWorker } from 'wrangler'
import { describe, expect, it, beforeAll, afterAll } from 'vitest'

describe('Worker', () => {
  let worker: UnstableDevWorker

  beforeAll(async () => {
    worker = await unstable_dev('src/index.ts', {
      experimental: { disableExperimentalWarning: true }
    })
  })

  afterAll(async () => {
    await worker.stop()
  })

  it('should ok', async () => {
    const resp = await worker.fetch()
    expect(resp.status).toBe(200)
    const text = await resp.text()
    expect(text).toBe('idempotent-proxy-cf-worker')
  })
})
