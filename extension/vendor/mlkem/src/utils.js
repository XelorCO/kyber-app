import * as dntShim from "../_dnt.shims.js";
import { shake256 } from "./deps.js";
export function byte(n) {
    return n & 0xFF;
}
export function int16(n) {
    return (n << 16) >> 16;
}
export function uint16(n) {
    return n & 0xFFFF;
}
export function int32(n) {
    return n | 0;
}
// any bit operations to be done in uint32 must have >>> 0
// javascript calculates bitwise in SIGNED 32 bit so you need to convert
export function uint32(n) {
    return n >>> 0;
}
/**
 * compares two arrays
 * @returns 1 if they are the same or 0 if not
 */
export function constantTimeCompare(x, y) {
    // check array lengths
    if (x.length != y.length) {
        return 0;
    }
    let v = 0;
    for (let i = 0; i < x.length; i++) {
        v |= x[i] ^ y[i];
    }
    // constantTimeByteEq
    let z = (~v) & 0xFF;
    z &= z >> 4;
    z &= z >> 2;
    z &= z >> 1;
    return z & 1;
}
export function equalUint8Array(x, y) {
    if (x.length != y.length) {
        return false;
    }
    for (let i = 0; i < x.length; i++) {
        if (x[i] !== y[i]) {
            return false;
        }
    }
    return true;
}
export async function loadCrypto() {
    if (typeof dntShim.dntGlobalThis !== "undefined" && globalThis.crypto !== undefined) {
        // Browsers, Node.js >= v19, Cloudflare Workers, Bun, etc.
        return globalThis.crypto;
    }
    // Node.js <= v18
    try {
        // @ts-ignore: to ignore "crypto"
        const { webcrypto } = await import("crypto"); // node:crypto
        return webcrypto;
    }
    catch (_e) {
        throw new Error("failed to load Crypto");
    }
}
// prf provides a pseudo-random function (PRF) which returns
// a byte array of length `l`, using the provided key and nonce
// to instantiate the PRF's underlying hash function.
export function prf(len, seed, nonce) {
    return shake256.create({ dkLen: len }).update(seed).update(new Uint8Array([nonce])).digest();
}
// byteopsLoad24 returns a 32-bit unsigned integer loaded from byte x at offset o.
export function byteopsLoad24(x, o = 0) {
    return x[o] | (x[o + 1] << 8) | (x[o + 2] << 16);
}
// byteopsLoad32 returns a 32-bit unsigned integer loaded from byte x at offset o.
export function byteopsLoad32(x, o = 0) {
    return (x[o] | (x[o + 1] << 8) | (x[o + 2] << 16) | (x[o + 3] << 24)) >>>
        0;
}
