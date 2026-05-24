# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Auto Apply LUT is a cross-platform desktop application for batch applying LUT (Look-Up Table) color corrections to video files. Built with Tauri (Rust backend + React frontend) using FFmpeg for video processing.

**Technology Stack:**
- Frontend: React 19 + TypeScript + Tailwind CSS
- Backend: Rust + Tauri v2
- Video Processing: FFmpeg
- Database: SQLite
- State Management: Zustand
- Build Tool: Vite

## Development Commands

### Environment Setup
```bash
# Install dependencies
pnpm install

# Install Tauri CLI (global)
pnpm install -g @tauri-apps/cli

# Windows only: Install MSVC toolchain
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

### Development
```bash
# Start development server (opens Tauri window automatically)
npm run tauri dev

# Frontend dev server only (port 1420)
npm run dev

# Build for production
npm run build

# Build Tauri application
npm run tauri build
```

### Code Quality
```bash
# Type checking
tsc --noEmit

# Tauri specific commands
npm run tauri [dev|build|info]
```

## Architecture Overview

### Frontend Structure
- **src/App.tsx**: Main application component with state management
- **src/components/**: React components (FileUpload, VideoPreview, SettingsModal, ProcessingStatus)
- **State**: Local React state for file selection, processing tasks, and settings
- **Tauri Integration**: Uses `@tauri-apps/api/core` for backend communication

### Backend Structure (Rust)

#### Entry Points
- **src-tauri/src/main.rs**: Application entry point
- **src-tauri/src/lib.rs**: Main Tauri setup and state managers

#### Core Modules
- **commands/**: Tauri command handlers (API layer)
  - `processor.rs`: Video processing commands
  - `lut_manager.rs`: LUT management operations
  - `file_manager.rs`: File operations
  - `system_manager.rs`: System information
  - `gpu_manager.rs`: GPU acceleration
  - `batch_manager.rs`: Batch processing

- **core/**: Business logic modules
  - `ffmpeg/`: Complete FFmpeg integration
  - `lut/`: LUT parsing, validation, processing
  - `video/`: Video metadata handling
  - `file/`: File operations and monitoring
  - `task/`: Task lifecycle management
  - `system/`: System resource monitoring
  - `gpu/`: GPU acceleration support

- **database/**: SQLite database operations and migrations
- **events/**: Real-time event system for frontend communication
- **types/**: Data type definitions
- **utils/**: Utility functions

### Key Components

#### Video Processing Pipeline
- FFmpeg-based video processing with LUT application
- Hardware acceleration support where available
- Real-time progress tracking via event system
- Background task processing with status management

#### LUT Management
- Multi-format LUT support (.cube, .3dl, .lut, .csp, etc.)
- LUT validation and metadata extraction
- LUT caching and conversion capabilities
- Grid size and range detection for 3D LUTs

#### Task Management
- Async background processing
- Task states: pending, running, completed, failed, cancelled
- Real-time progress updates via events
- Task cancellation and retry mechanisms

## Key Patterns

### State Management
- Frontend: React local state with callbacks
- Backend: Tauri state managers (LutManager, TaskManager, VideoProcessor, DatabaseManager)
- Communication: Tauri commands + event broadcasting

### Error Handling
- Comprehensive error types (18+ categories)
- `AppResult<T>` wrapper for consistent error handling
- Chinese error messages throughout codebase

### File Handling
- File selection through native dialogs
- Drag-and-drop support in frontend
- File metadata extraction and validation
- Temporary file management for processing

## Important Notes

### FFmpeg Integration
- Application requires FFmpeg to be installed on the system
- Auto-discovers FFmpeg in common installation paths
- Falls back to bundled FFmpeg if available

### Database
- Uses SQLite with bundled version (no external dependency)
- Includes migration system for schema updates
- Stores video metadata, LUT information, task history, and settings

### Event System
- Real-time communication between backend and frontend
- Events: Progress, Error, TaskCompleted, SystemStatus, GpuStatus, BatchStatus
- Used for UI updates during long-running operations

### Development Notes
- Chinese comments and error messages throughout codebase
- Tauri window runs on localhost:1420 during development
- Hot reload enabled for frontend changes
- Rust backend requires recompilation for changes

### Testing
- No formal test suite currently implemented
- Manual testing through development mode
- Focus on cross-platform compatibility (Windows, macOS, Linux)

## Configuration Files

- **src-tauri/tauri.conf.json**: Tauri application configuration
- **vite.config.ts**: Frontend build configuration with React plugin
- **tsconfig.json**: TypeScript configuration
- **tailwind.config.js**: Tailwind CSS configuration
- **src-tauri/Cargo.toml**: Rust dependencies and configuration