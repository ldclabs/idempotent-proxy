import { sha3_256 as sha3 } from '@noble/hashes/sha3'
import { ed25519 } from '@noble/curves/ed25519'
import { encode, decode } from 'cborg'
import { secp256k1 } from '@noble/curves/secp256k1'

const PERMITTED_DRIFT = 10 // seconds

export type Token = [number, string, Uint8Array]

export function ed25519Sign(
  privateKey: Uint8Array,
  expire_at: number,
  message: String
): Uint8Array {
  const sig = ed25519.sign(encode([expire_at, message]), privateKey)
  return encode([expire_at, message, sig])
}

export function ed25519Verify(
  pubKeys: Array<Uint8Array>,
  data: Uint8Array
): Token {
  const token: Token = decode(data)
  if (token[0] + PERMITTED_DRIFT < Date.now() / 1000) {
    throw new Error('token expired')
  }

  const msg = encode(token.slice(0, 2))
  for (const pubKey of pubKeys) {
    if (ed25519.verify(token[2], msg, pubKey)) {
      return token
    }
  }

  throw new Error('failed to verify Ed25519 signature')
}

export function ecdsaSign(
  privateKey: Uint8Array,
  expire_at: number,
  message: String
): Uint8Array {
  const digest = sha3(encode([expire_at, message]))
  const sig = secp256k1.sign(digest, privateKey)
  return encode([expire_at, message, sig])
}

export function ecdsaVerify(
  pubKeys: Array<Uint8Array>,
  data: Uint8Array
): Token {
  const token: Token = decode(data)
  if (token[0] + PERMITTED_DRIFT < Date.now() / 1000) {
    throw new Error('token expired')
  }

  const digest = sha3(encode(token.slice(0, 2)))
  for (const pubKey of pubKeys) {
    if (secp256k1.verify(token[2], digest, pubKey)) {
      return token
    }
  }

  throw new Error('failed to verify ECDSA/secp256k1 signature')
}

export function bytesToBase64Url(bytes: Uint8Array): string {
  return btoa(String.fromCodePoint(...bytes))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '')
}

export function base64ToBytes(str: string): Uint8Array {
  return Uint8Array.from(
    atob(str.replaceAll('-', '+').replaceAll('_', '/')),
    (m) => m.codePointAt(0)!
  )
}
