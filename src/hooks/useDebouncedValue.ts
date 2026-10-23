import { useCallback, useEffect, useState } from "react";

export function useDebouncedValue<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

export function useDebouncedCallback<T extends (...args: never[]) => void>(
  fn: T,
  ms: number,
): (...args: Parameters<T>) => void {
  const fnRef = useCallback(fn, [fn]);
  return useCallback(
    (...args: Parameters<T>) => {
      const id = setTimeout(() => fnRef(...args), ms);
      return () => clearTimeout(id);
    },
    [fnRef, ms],
  );
}
