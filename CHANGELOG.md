# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.7] - 2025-12-17

### Added
- Job cancellation infrastructure for workers with abort signal support
- Comprehensive Maven jobs solution documentation
- Working examples for asdf-based Java and Maven installation
- Docker provider solution for complex Maven builds

### Changed
- Updated GETTING_STARTED.md with clear Maven job examples
- Improved worker cancellation handling in gRPC services
- Enhanced documentation with inline JSON payload examples

### Fixed
- Resolved worker file access limitations by using gRPC inline payloads
- Fixed asdf commands to use 'set' instead of deprecated 'global'
- Improved worker provisioning for long-running operations

### Verified
- ✅ Java installation with asdf: WORKING
- ✅ Maven installation with asdf: WORKING  
- ✅ Git operations in workers: WORKING
- ✅ Docker provider with maven images: WORKING
- ✅ Streaming logs functionality: WORKING

### Status
**PRODUCTION READY** - All job types supported with validated workflows

## [0.1.6] - 2025-12-17

### Fixed
- Stabilized production code with critical bug fixes
- Improved worker provider configuration

## [0.1.5] - 2025-12-17

### Changed
- Bumped version for release cycle
- Updated infrastructure configuration

## [0.1.4] - 2025-12-17

### Added
- Initial release with core job execution platform
- gRPC-based job queuing and execution
- Docker and process-based worker providers
- Comprehensive audit logging system
