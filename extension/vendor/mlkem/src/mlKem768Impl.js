import { N } from "./consts.js";
import { MlKemError } from "./errors.js";
import { MlKemBase } from "./mlKemBase.js";
/**
 * Synchronous implementation of MlKem768.
 *
 * Use {@link createMlKem768} to create an initialized instance.
 *
 * @example
 *
 * ```ts
 * // Using jsr:
 * import { createMlKem768 } from "@dajiaji/mlkem";
 * // Using npm:
 * // import { createMlKem768 } from "mlkem"; // or "crystals-kyber-js"
 *
 * const recipient = await createMlKem768();
 * const [pkR, skR] = recipient.generateKeyPair();
 *
 * const sender = await createMlKem768();
 * const [ct, ssS] = sender.encap(pkR);
 *
 * const ssR = recipient.decap(ct, skR);
 * // ssS === ssR
 * ```
 */
export class MlKem768Impl extends MlKemBase {
    constructor() {
        super();
        Object.defineProperty(this, "_k", {
            enumerable: true,
            configurable: true,
            writable: true,
            value: 3
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
            value: 2
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
    /** Generates a keypair [publicKey, privateKey]. */
    generateKeyPair() {
        try {
            return this._generateKeyPairCore();
        }
        catch (e) {
            throw new MlKemError(e);
        }
    }
    /** Derives a keypair [publicKey, privateKey] deterministically from a 64-octet seed. */
    deriveKeyPair(seed) {
        try {
            return this._deriveKeyPairCore(seed);
        }
        catch (e) {
            throw new MlKemError(e);
        }
    }
    /** Encapsulates: returns [ciphertext, sharedSecret]. */
    encap(pk, seed) {
        try {
            return this._encapCore(pk, seed);
        }
        catch (e) {
            throw new MlKemError(e);
        }
    }
    /** Decapsulates: returns sharedSecret. */
    decap(ct, sk) {
        try {
            return this._decapCore(ct, sk);
        }
        catch (e) {
            throw new MlKemError(e);
        }
    }
    /** @internal */
    static async _create() {
        const impl = new MlKem768Impl();
        await impl._setup();
        return impl;
    }
}
/**
 * Creates a pre-initialized MlKem768 instance with synchronous operations.
 *
 * @returns A promise that resolves to an {@link MlKemInterface} instance.
 *
 * @example
 *
 * ```ts
 * const ctx = await createMlKem768();
 * const [pk, sk] = ctx.generateKeyPair();       // sync
 * const [ct, ssS] = ctx.encap(pk);              // sync
 * const ssR = ctx.decap(ct, sk);                // sync
 * ```
 */
export function createMlKem768() {
    return MlKem768Impl._create();
}
