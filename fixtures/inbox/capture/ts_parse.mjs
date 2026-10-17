// Run the herdr-inbox-ui mod's own `[inbox]` parsers (node >= 23.6, which strips TS types).
//
//   node ts_parse.mjs captured <captured.ts> <out.json>      the mod's captured Claude Code inputs
//   node ts_parse.mjs parse <register.tsx> <in.json> <out.json>
//
// register.tsx is TSX (its renderers need the claude-code runtime), so the pure functions are
// cut out of it verbatim: `clean`, `parseAnswers` and `rewakeAnswers` with the `Answer` type,
// written to a .ts module next to <out.json> and imported. Nothing is re-implemented here.
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [mode, src, a, b] = process.argv.slice(2)

function cut(text, start) {
  const at = text.indexOf(start)
  if (at < 0) throw new Error(`not found in register.tsx: ${start}`)
  let depth = 0
  for (let i = text.indexOf('{', at); i < text.length; i++) {
    if (text[i] === '{') depth++
    else if (text[i] === '}' && --depth === 0) return text.slice(at, i + 1)
  }
  throw new Error(`unbalanced: ${start}`)
}

function line(text, start) {
  const at = text.indexOf(start)
  if (at < 0) throw new Error(`not found in register.tsx: ${start}`)
  return text.slice(at, text.indexOf('\n', at))
}

if (mode === 'captured') {
  const mod = await import(pathToFileURL(resolve(src)).href)
  writeFileSync(a, JSON.stringify(mod, null, 1))
} else if (mode === 'parse') {
  const tsx = readFileSync(src, 'utf8')
  const module = [
    line(tsx, 'type Answer = '),
    line(tsx, 'const clean = '),
    cut(tsx, 'export function parseAnswers('),
    cut(tsx, 'export function rewakeAnswers('),
  ].join('\n\n') + '\n'
  const file = join(dirname(resolve(b)), 'register-parsers.ts')
  writeFileSync(file, module)
  const { parseAnswers, rewakeAnswers } = await import(pathToFileURL(file).href)
  const inputs = JSON.parse(readFileSync(a, 'utf8'))
  writeFileSync(b, JSON.stringify(inputs.map(text => ({ text, answers: parseAnswers(text), rewake_answers: rewakeAnswers(text) })), null, 1))
} else {
  throw new Error(`unknown mode ${mode}`)
}
