"""Parallel execution utility with automatic fallback to single-threaded mode.

Provides a ThreadPoolExecutor wrapper for parallelizing script operations
(e.g., ngspice subprocess calls or render303 invocations). If any parallel
task raises an exception, the remaining futures are cancelled and execution
falls back to processing sequentially in the main thread.
"""

import concurrent.futures
import sys
import threading
from typing import Any, Callable, Iterable


def run_parallel_with_fallback(
    func: Callable[..., Any],
    tasks: Iterable[tuple | Any],
    desc: str = "Tasks",
    max_workers: int = 4,
) -> list[Any]:
    """
    Run `func` across `tasks` concurrently using a ThreadPoolExecutor.
    If an exception occurs during parallel execution, emits a warning and falls back
    to running all tasks sequentially.

    Args:
        func: The target function to call.
        tasks: An iterable of arguments to pass to `func`. If an element is a tuple,
               it is unpacked as `*args`. Otherwise, it is passed as a single argument.
        desc: Description used for logging.
        max_workers: Maximum number of threads (defaults to 4).

    Returns:
        A list of results corresponding to the order of `tasks`.
    """
    tasks_list = list(tasks)
    if not tasks_list:
        return []

    if max_workers <= 1:
        # Fast path if explicitly single-threaded
        return _run_sequential(func, tasks_list)

    results: list[Any] = [None] * len(tasks_list)
    lock = threading.Lock()
    completed = 0
    total = len(tasks_list)

    def print_progress():
        nonlocal completed
        with lock:
            completed += 1
            print(f"\r{desc} {completed}/{total}...", end="", flush=True)

    def worker(idx: int, arg: Any) -> tuple[int, Any]:
        args = arg if isinstance(arg, tuple) else (arg,)
        res = func(*args)
        print_progress()
        return idx, res

    print(f"{desc} (parallel, {max_workers} workers)... ", end="", flush=True)
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:
            # Submit all tasks and keep track of index to maintain original order
            future_to_idx = {
                executor.submit(worker, i, arg): i
                for i, arg in enumerate(tasks_list)
            }

            for future in concurrent.futures.as_completed(future_to_idx):
                idx, res = future.result()  # Re-raises any exception caught by worker
                results[idx] = res

        print()  # Clear progress line
        return results

    except Exception as e:
        print(f"\n[Warning] Parallel execution failed for '{desc}': {e}", file=sys.stderr)
        print("Falling back to single-threaded sequential execution...", file=sys.stderr)
        return _run_sequential(func, tasks_list, desc)


def _run_sequential(
    func: Callable[..., Any], tasks: list[Any], desc: str = ""
) -> list[Any]:
    results = []
    total = len(tasks)
    for i, arg in enumerate(tasks):
        if desc:
            print(f"\r{desc} (sequential) {i + 1}/{total}...", end="", flush=True)
        args = arg if isinstance(arg, tuple) else (arg,)
        results.append(func(*args))
    if desc:
        print()
    return results
