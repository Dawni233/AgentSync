/** 行级 diff 结果行 */
export type DiffLine = { type: 'same' | 'add' | 'del'; text: string }

/**
 * 行级 diff（基于 LCS 动态规划）。
 * @param a 基准侧（人格快照内容），对应 diff 中的 `-` 行
 * @param b 对照侧（本地内容），对应 diff 中的 `+` 行
 *
 * 边界：
 * - a 与 b 都为空 -> 返回 []
 * - a 为空 -> 全部为 add 行
 * - b 为空 -> 全部为 del 行
 * - a === b -> 全部为 same 行
 */
export function lineDiff(a: string, b: string): DiffLine[] {
  const linesA = a.length === 0 ? [] : a.split('\n')
  const linesB = b.length === 0 ? [] : b.split('\n')

  const m = linesA.length
  const n = linesB.length

  // dp[i][j] = linesA[0..i) 与 linesB[0..j) 的 LCS 长度
  const dp: number[][] = Array.from({ length: m + 1 }, () =>
    new Array(n + 1).fill(0)
  )
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (linesA[i - 1] === linesB[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1])
      }
    }
  }

  // 回溯生成 diff 序列
  const result: DiffLine[] = []
  let i = m
  let j = n
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && linesA[i - 1] === linesB[j - 1]) {
      result.push({ type: 'same', text: linesA[i - 1] })
      i--
      j--
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      result.push({ type: 'add', text: linesB[j - 1] })
      j--
    } else {
      result.push({ type: 'del', text: linesA[i - 1] })
      i--
    }
  }
  result.reverse()
  return result
}
