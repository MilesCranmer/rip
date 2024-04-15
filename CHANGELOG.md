# Changelog

## [0.4.0](https://github.com/MilesCranmer/rip2/compare/v0.3.0...v0.4.0) (2024-04-15)


### ⚠ BREAKING CHANGES

* do not record permanent deletions in record
* use dunce canonicalization for windows compat

### Features

* add preliminary windows support ([51bcdf3](https://github.com/MilesCranmer/rip2/commit/51bcdf3e0143858b0e17ea1a31fbaa6b3a90683c))
* do not record permanent deletions in record ([a77e027](https://github.com/MilesCranmer/rip2/commit/a77e027c383af922fec1eeda4eb855b5f82d3bbf))
* more readable logging for windows ([f494d9e](https://github.com/MilesCranmer/rip2/commit/f494d9e3b45210b74ab55a9efc6792e321912a43))
* quit prompt read if given invalid char ([51b0dcf](https://github.com/MilesCranmer/rip2/commit/51b0dcfc4fddca4e799895053d2b68f913ca6371))


### Bug Fixes

* correct behavior for \n stdin ([5c60870](https://github.com/MilesCranmer/rip2/commit/5c608704a16ff36d143a665d2789da3bc67a692f))
* correct behavior for non-input stdin ([b4035a4](https://github.com/MilesCranmer/rip2/commit/b4035a4c240a839cfe3c25607fef07edf2463912))
* correct symlink to symlink_file on windows ([d1ca9ca](https://github.com/MilesCranmer/rip2/commit/d1ca9ca27e35a9dd45c40d31785d76d18820a675))
* seance paths on windows ([9c0d2d5](https://github.com/MilesCranmer/rip2/commit/9c0d2d516fa4146dcb2971a6482b75dfd7f23d59))
* use dunce canonicalization for windows compat ([0d3dc2a](https://github.com/MilesCranmer/rip2/commit/0d3dc2abe6086f7c8460c7552a9cc610ed07bb49))
* workaround for device paths on windows ([6624147](https://github.com/MilesCranmer/rip2/commit/66241479e0f95793b167dc186175e533e4e351c0))

## [0.3.0](https://github.com/MilesCranmer/rip2/compare/v0.2.1...v0.3.0) (2024-04-14)


### ⚠ BREAKING CHANGES

* use subcommands for shell completions

### Features

* use subcommands for shell completions ([adbb270](https://github.com/MilesCranmer/rip2/commit/adbb270190a80a33515b091d50f8c0455029c9c6))


### Bug Fixes

* correct output of shell completions ([67ee0df](https://github.com/MilesCranmer/rip2/commit/67ee0dfb44ae518c68113c857aea093bbf2de62b))

## [0.2.1](https://github.com/MilesCranmer/rip2/compare/v0.2.0...v0.2.1) (2024-04-11)


### Bug Fixes

* flush stream even if not stdout ([09504c8](https://github.com/MilesCranmer/rip2/commit/09504c8b8d16d07aa973ace093b80485a87ee32e))

## [0.2.0](https://github.com/MilesCranmer/rip2/compare/v0.1.0...v0.2.0) (2024-04-09)


### Features

* test feat ([11656a2](https://github.com/MilesCranmer/rip2/commit/11656a2c3216fed0dc6b3a4566641d8c571bf107))
