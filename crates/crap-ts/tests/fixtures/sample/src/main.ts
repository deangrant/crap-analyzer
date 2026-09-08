export function trivial(): number {
  const x = 1;
  return x;
}

export function branched(x: number): number {
  if (x > 0) {
    return x;
  }
  return 0;
}
