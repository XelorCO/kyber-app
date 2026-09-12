/**
 * This implementation is based on https://github.com/antontutoveanu/crystals-kyber-javascript,
 * which was deveploped under the MIT licence below:
 * https://github.com/antontutoveanu/crystals-kyber-javascript/blob/main/LICENSE
 */
import { N } from "./consts.js";
import { MlKemBase } from "./mlKemBase.js";
import { byteopsLoad24, int16 } from "./utils.js";
/**
 * Shared base for MlKem512 and MlKem512Impl.
 * Contains parameter configuration and the _sampleNoise1 override.
 */
export class MlKem512Base extends MlKemBase {
    constructor() {
        super();
        Object.defineProperty(this, "_k", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 2
        });
        Object.defineProperty(this, "_du", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 10
        });
        Object.defineProperty(this, "_dv", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 4
        });
        Object.defineProperty(this, "_eta1", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 3
        });
        Object.defineProperty(this, "_eta2", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 2
        });
        this._skSize = 12 * this._k * N / 8;
        this._pkSize = this._skSize + 32;
        this._compressedUSize = this._k * this._du * N / 8;
        this._compressedVSize = this._dv * N / 8;
        this._initPool();
    }
    /**
     * Samples a vector of polynomials from a seed.
     * @internal
     * @param sigma - The seed.
     * @param offset - The offset.
     * @param size - The size.
     * @returns The sampled vector of polynomials.
     */
    _sampleNoise1(sigma, offset, size) {
        const r = new Array(size);
        for (let i = 0; i < size; i++) {
            r[i] = this._noiseVecs[offset + i];
            byteopsCbd(r[i], this._prf1(sigma, offset + i), this._eta1);
        }
        return r;
    }
}
/**
 * Performs the byte operations for the Cbd function.
 *
 * @param out - The output array to write into.
 * @param buf - The input buffer.
 * @param eta - The value of eta.
 */
function byteopsCbd(out, buf, eta) {
    let t, d;
    let a, b;
    for (let i = 0; i < N / 4; i++) {
        t = byteopsLoad24(buf, 3 * i);
        d = t & 0x00249249;
        d = d + ((t >> 1) & 0x00249249);
        d = d + ((t >> 2) & 0x00249249);
        for (let j = 0; j < 4; j++) {
            a = int16((d >> (6 * j + 0)) & 0x7);
            b = int16((d >> (6 * j + eta)) & 0x7);
            out[4 * i + j] = a - b;
        }
    }
}
