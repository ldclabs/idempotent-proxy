/**
 * Welcome to Cloudflare Workers! This is your first worker.
 *
 * - Run `npm run dev` in your terminal to start a development server
 * - Open a browser tab at http://localhost:8787/ to see your worker in action
 * - Run `npm run deploy` to publish your worker
 *
 * Learn more at https://developers.cloudflare.com/workers/
 */

import { DurableObject } from 'cloudflare:workers'
import { EnvVars, ResponseData, proxyRequestHeaders } from './util'

const HEADER_PROXY_AUTHORIZATION = 'proxy-authorization'
const HEADER_X_FORWARDED_HOST = 'x-forwarded-host'
const HEADER_IDEMPOTENCY_KEY = 'idempotency-key'
const HEADER_X_JSON_MASK = 'x-json-mask'
const HEADER_RESPONSE_HEADERS = 'response-headers'

export interface Env {
  POLL_INTERVAL: number // in milliseconds
  REQUEST_TIMEOUT: number // in milliseconds
  ALLOW_AGENTS: string[]
  MY_DURABLE_OBJECT: DurableObjectNamespace
  CACHER: DurableObjectNamespace<Cacher>
}

// Worker
export default {
  async fetch(
    req: Request,
    env: Env,
    _ctx: ExecutionContext
  ): Promise<Response> {
    const ev = new EnvVars(env)
    let agent = 'ANON'
    if (ev.parsePubkeys()) {
      agent = ev.verifyToken(req.headers.get(HEADER_PROXY_AUTHORIZATION) || '')
    }

    if (env.ALLOW_AGENTS.length > 0 && !env.ALLOW_AGENTS.includes(agent)) {
      return new Response(`agent ${agent} is not allowed`, { status: 403 })
    }

    let url = new URL(req.url)
    if (req.method == 'GET' && url.pathname == '/') {
      return new Response('idempotent-proxy-cf-worker', {
        headers: { 'content-type': 'text/plain' }
      })
    }

    if (url.pathname.startsWith('/URL_')) {
      url = new URL(ev.getString(url.pathname.slice(1)))
    } else {
      const host = req.headers.get(HEADER_X_FORWARDED_HOST)
      if (!host) {
        return new Response('missing header: ' + HEADER_X_FORWARDED_HOST, {
          status: 400
        })
      }
      url.port = ''
      url.protocol = 'https'
      url.host = host
    }

    const idempotencyKey = req.headers.get(HEADER_IDEMPOTENCY_KEY)
    if (!idempotencyKey) {
      return new Response('missing header: ' + HEADER_IDEMPOTENCY_KEY, {
        status: 400
      })
    }

    const id = env.CACHER.idFromName(`${agent}:${req.method}:${idempotencyKey}`)
    const stub = env.CACHER.get(id)

    try {
      const lock = await stub.obtain()
      if (!lock) {
        const data = await polling_get(
          stub,
          env.POLL_INTERVAL,
          Math.floor(env.REQUEST_TIMEOUT / env.POLL_INTERVAL)
        )
        const rd = ResponseData.fromBytes(data)
        return rd.toResponse()
      }

      const res = await fetch(url, {
        method: req.method,
        headers: proxyRequestHeaders(req.headers, ev),
        body: req.body
      })

      if (res.status >= 200 && res.status <= 500) {
        const data = await res.arrayBuffer()
        const rd = new ResponseData(res.status)
          .setHeaders(
            new Headers(res.headers),
            req.headers.get(HEADER_RESPONSE_HEADERS) || ''
          )
          .setBody(
            new Uint8Array(data),
            req.headers.get(HEADER_X_JSON_MASK) || ''
          )
        await stub.set(rd.toBytes())
        return rd.toResponse()
      }

      stub.del()
      return new Response(await res.text(), {
        status: res.status
      })
    } catch (err) {
      stub.del()
      return new Response(String(err), { status: 500 })
    }
  }
}

async function polling_get(
  stub: DurableObjectStub<Cacher>,
  poll_interval: number,
  counter: number
): Promise<Uint8Array> {
  while (counter > 0) {
    const value = await stub.get()
    if (value) {
      return value
    }

    counter -= 1
    await new Promise((resolve) => setTimeout(resolve, poll_interval))
  }

  throw new Error('polling get cache timeout')
}

// Durable Object
export class Cacher extends DurableObject {
  private readonly ttl: number
  private status: number // 0, 1, 2

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env)
    this.status = 0
    this.ttl = env.REQUEST_TIMEOUT || 10 * 1000

    this.ctx.blockConcurrencyWhile(async () => {
      this.status = (await this.ctx.storage.get('s')) || 0
    })
    if (this.ctx.storage.getAlarm() == null) {
      this.ctx.storage.setAlarm(Date.now() + this.ttl)
    }
  }

  async obtain(): Promise<boolean> {
    if (this.status == 0) {
      this.status = 1
      this.ctx.storage.put('s', this.status)
      return true
    }
    return false
  }

  async get(): Promise<Uint8Array | null> {
    if (this.status == 0) {
      throw new Error('not obtained')
    } else if (this.status != 2) {
      return null
    }
    return (await this.ctx.storage.get('v')) || null
  }

  async set(value: Uint8Array): Promise<void> {
    this.status = 2
    this.ctx.storage.setAlarm(Date.now() + this.ttl)
    await Promise.all([
      this.ctx.storage.put('s', this.status),
      this.ctx.storage.put('v', value)
    ])
  }

  async del(): Promise<void> {
    this.status = 0
    await Promise.all([
      this.ctx.storage.deleteAlarm(),
      this.ctx.storage.deleteAll()
    ])
  }

  async alarm() {
    this.del()
  }
}
