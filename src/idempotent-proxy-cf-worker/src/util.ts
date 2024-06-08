import { base64ToBytes, ed25519Verify, ecdsaVerify } from './auth'
import { encode, decode } from 'cborg'

export interface Pubkeys {
  ecdsa: Array<Uint8Array> // ECDSA/secp256k1
  ed25519: Array<Uint8Array>
}

export class EnvVars {
  private _env: any
  private _pubKeys: Pubkeys

  constructor(env: any = {}) {
    this._env = env
    this._pubKeys = {
      ecdsa: [],
      ed25519: []
    }
  }

  getString(key: string): string {
    return this._env[key] || ''
  }

  parsePubkeys(): boolean {
    for (const [key, value] of this._env) {
      if (key.startsWith('ECDSA_PUB_KEY')) {
        this._pubKeys.ecdsa.push(base64ToBytes(value))
      } else if (key.startsWith('ED25519_PUB_KEY')) {
        this._pubKeys.ed25519.push(base64ToBytes(value))
      }
    }

    return this._pubKeys.ecdsa.length > 0 || this._pubKeys.ed25519.length > 0
  }

  verifyToken(token: string) {
    if (!token.startsWith('Bearer ')) {
      throw new Error('invalid bearer token')
    }

    const data = base64ToBytes(token.slice(7))
    if (this._pubKeys.ecdsa.length > 0) {
      return ecdsaVerify(this._pubKeys.ecdsa, data)
    } else if (this._pubKeys.ed25519.length > 0) {
      return ed25519Verify(this._pubKeys.ecdsa, data)
    }

    throw new Error('no public key found')
  }
}

export class ResponseData {
  status: number
  headers: Array<[string, string]>
  body: Uint8Array | null
  mime: string

  static fromBytes(data: Uint8Array): ResponseData {
    const obj = decode(data)
    const rd = new ResponseData(obj.status)
    rd.headers = obj.headers
    rd.body = obj.body
    rd.mime = obj.mime
    return rd
  }

  constructor(status: number = 200) {
    this.status = status
    this.headers = []
    this.body = null
    this.mime = 'text/plain'
  }

  setHeaders(headers: Headers, filtering: string): this {
    const fi = filtering
      .toLocaleLowerCase()
      .split(',')
      .map((v) => v.trim())
      .filter((v) => v.length > 0)
    for (const [key, value] of headers) {
      if (key == 'content-type') {
        this.mime = value
      } else if (
        key != 'content-length' &&
        (fi.length == 0 || fi.includes(key))
      ) {
        this.headers.push([key, value])
      }
    }
    return this
  }

  setBody(body: Uint8Array, filtering: string): this {
    const fi = filtering
      .split(',')
      .map((v) => v.trim())
      .filter((v) => v.length > 0)
    if (fi.length > 0 && this.status < 300 && this.mime.includes('application/json')) {
      const decoder = new TextDecoder()
      const str = decoder.decode(body)
      const obj = JSON.parse(str)
      const newObj = {} as any
      for (const key of fi) {
        if (Object.hasOwn(obj, key)) {
          newObj[key] = obj[key]
        }
      }

      this.body = new TextEncoder().encode(JSON.stringify(newObj))
    } else if (fi.length > 0 && this.status < 300 && this.mime.includes('application/cbor')) {
      const obj = decode(body)
      const newObj = {} as any
      for (const key of fi) {
        if (Object.hasOwn(obj, key)) {
          newObj[key] = obj[key]
        }
      }
      this.body = encode(newObj)
    } else {
      this.body = body
    }

    return this
  }

  toBytes(): Uint8Array {
    return encode({
      headers: this.headers,
      body: this.body,
      mime: this.mime,
      status: this.status
    })
  }

  toResponse(): Response {
    const headers = new Headers(this.headers)
    headers.set('content-type', this.mime)

    return new Response(this.body, {
      headers,
      status: 200
    })
  }
}

const filterHeaders = [
  'host',
  'forwarded',
  'proxy-authorization',
  'x-forwarded-for',
  'x-forwarded-host',
  'x-forwarded-proto',
  'idempotency-key'
]

export function proxyRequestHeaders(headers: Headers, ev: EnvVars): Headers {
  const newHeaders = new Headers()
  for (const [key, value] of headers) {
    if (!filterHeaders.includes(key)) {
      newHeaders.append(
        key,
        value.startsWith('HEADER_') ? ev.getString(value) : value
      )
    }
  }
  return newHeaders
}
