// Picks between the single-threaded (`pkg/`) and multi-threaded (`pkg-mt/`)
// builds of demystify-wasm at runtime, and initialises whichever it chose.
//
// The two builds cannot be merged into one file: the threaded module imports a
// *shared* memory, which needs a SharedArrayBuffer, which needs the page to be
// cross-origin isolated.  Instantiating it on a non-isolated page fails
// outright, so the choice has to happen before the module is fetched.
//
// Using this loader is optional -- importing `./pkg/demystify_wasm.js`
// directly still works exactly as before.  It exists to stop callers getting
// two things wrong, both of which fail badly:
//
//   1. Ordering.  In the threaded build the worker pool must be running before
//      the first WasmPlanner is constructed; missing that is not recoverable
//      for the lifetime of the page.
//   2. Thread of execution.  The threaded build blocks on a mutex during MUS
//      search.  The browser main thread may not block, so solving there hangs
//      indefinitely -- no exception, nothing in the console.  Threads are
//      opt-in *and* Worker-only for this reason; see `threadsAvailable`.
//
// Note for bundler users: the dynamic `import()` below takes a computed
// specifier, which webpack/Vite cannot follow statically.  Both packages are
// built with wasm-pack's `--target web`; if you bundle, import the one you want
// directly instead of using this loader.

/**
 * True when this context *could* run the threaded build.  Three conditions,
 * all required: `crossOriginIsolated` reflects the COOP/COEP headers,
 * `SharedArrayBuffer` is what the shared memory is built on, and `window`
 * being absent means we are inside a Worker rather than on the main thread.
 *
 * Note the third condition.  The threaded build blocks on a mutex during MUS
 * search, and the browser main thread is not allowed to block, so calling it
 * there hangs forever with no error at all.  Isolation alone is NOT enough.
 */
export function threadsAvailable() {
  return (
    typeof SharedArrayBuffer !== 'undefined' &&
    globalThis.crossOriginIsolated === true &&
    typeof globalThis.window === 'undefined'
  );
}

/**
 * Loads, initialises and returns the appropriate demystify-wasm build.
 *
 * @param {object}  [opts]
 * @param {string}  [opts.st]       URL of the single-threaded package entry.
 * @param {string}  [opts.mt]       URL of the multi-threaded package entry.
 * @param {boolean} [opts.threads]  Force threaded on/off instead of detecting.
 *                                  Forcing `true` on a non-isolated page will
 *                                  throw rather than silently fall back.
 * @param {number}  [opts.numThreads] Worker count; defaults to a capped
 *                                  `navigator.hardwareConcurrency`.
 * @returns {Promise<{pkg: object, threads: number}>} `threads` is 1 for the
 *          single-threaded build, otherwise the worker count in use.
 */
export async function loadDemystify(opts = {}) {
  const {
    st = './pkg/demystify_wasm.js',
    mt = './pkg-mt/demystify_wasm.js',
    threads,
    numThreads,
  } = opts;

  // Defaults to OFF.  Auto-enabling would be actively dangerous: on any
  // cross-origin-isolated page, a caller who solves synchronously from the main
  // thread would hang with no diagnostic.  Threads are therefore something you
  // ask for, from a Worker, having decided your call site can block.
  const useThreads = threads === undefined ? false : threads;

  if (!useThreads) {
    const pkg = await import(st);
    await pkg.default();
    return { pkg, threads: 1 };
  }

  if (!threadsAvailable()) {
    // Deliberately not a silent fallback to `st`: the caller asked for threads
    // explicitly, and quietly running ~N times slower is the kind of thing that
    // gets discovered months later.
    const reason =
      typeof globalThis.window !== 'undefined'
        ? 'this is the browser main thread, which is not allowed to block -- ' +
          'the threaded build must be loaded from inside a Web Worker'
        : 'this context is not cross-origin isolated -- serve the page with ' +
          '"Cross-Origin-Opener-Policy: same-origin" and ' +
          '"Cross-Origin-Embedder-Policy: require-corp"';
    throw new Error(
      `demystify-wasm: threads requested but ${reason}. Omit \`threads\` to use ` +
        'the single-threaded build, which has no such restriction.',
    );
  }

  // Capped rather than using hardwareConcurrency raw: every rayon worker holds
  // its own SAT solver instance, and the module's shared memory is capped at
  // 1 GiB, so a 32-core machine would multiply solver memory 32-fold for very
  // little extra parallelism.
  const n =
    numThreads ?? Math.min(globalThis.navigator?.hardwareConcurrency || 4, 8);

  const pkg = await import(mt);
  await pkg.default();
  // Must happen before any WasmPlanner is constructed -- constructing one
  // touches rayon, which permanently installs a single-threaded fallback pool
  // and makes initThreadPool fail for the lifetime of the page.
  await pkg.initThreadPool(n);
  return { pkg, threads: n };
}
