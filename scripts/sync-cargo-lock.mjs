// release-it `after:bump` hook. Bumper rewrites tauri.conf.json and
// Cargo.toml, but cargo does not eagerly sync workspace-member versions in
// Cargo.lock, so keep the `veilapp` entry in line with package.json here.
import { readFileSync, writeFileSync } from 'node:fs'

const version = JSON.parse(readFileSync('package.json', 'utf8')).version
const path = 'src-tauri/Cargo.lock'
const lock = readFileSync(path, 'utf8')
const updated = lock.replace(
  /name = "veilapp"\nversion = "[^"]+"/,
  `name = "veilapp"\nversion = "${version}"`
)
if (updated === lock) {
  console.error(`veilapp entry not found in ${path} or already at ${version}`)
  process.exit(1)
}
writeFileSync(path, updated)
console.log(`Cargo.lock: veilapp -> ${version}`)
