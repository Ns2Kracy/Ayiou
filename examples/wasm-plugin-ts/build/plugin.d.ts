declare namespace __AdaptedExports {
  /** Exported memory */
  export const memory: WebAssembly.Memory;
  // Exported runtime interface
  export function __new(size: number, id: number): number;
  export function __pin(ptr: number): number;
  export function __unpin(ptr: number): void;
  export function __collect(): void;
  export const __rtti_base: number;
  /**
   * assembly/index/ayiou_alloc
   * @param size `i32`
   * @returns `i32`
   */
  export function ayiou_alloc(size: number): number;
  /**
   * assembly/index/ayiou_free
   * @param ptr `i32`
   */
  export function ayiou_free(ptr: number): void;
  /**
   * assembly/index/ayiou_meta
   * @returns `i32`
   */
  export function ayiou_meta(): number;
  /**
   * assembly/index/ayiou_matches
   * @param ctx_ptr `i32`
   * @param ctx_len `i32`
   * @returns `i32`
   */
  export function ayiou_matches(ctx_ptr: number, ctx_len: number): number;
  /**
   * assembly/index/ayiou_handle
   * @param ctx_ptr `i32`
   * @param ctx_len `i32`
   * @returns `i32`
   */
  export function ayiou_handle(ctx_ptr: number, ctx_len: number): number;
}
/** Instantiates the compiled WebAssembly module with the given imports. */
export declare function instantiate(module: WebAssembly.Module, imports: {
  env: unknown,
}): Promise<typeof __AdaptedExports>;
