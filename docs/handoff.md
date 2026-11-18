# Aeri Browser — Product & Engineering Handoff

**Project:** Aeri Browser
**Application Name:** Aeri
**Project Type:** Open-source desktop web browser
**Primary Platform:** macOS (Apple Silicon first)
**Secondary Platform:** Windows
**Primary Language:** Rust
**UI Framework:** GPUI
**Web Engine:** Chromium via Chromium Embedded Framework (CEF)
**Status:** Initial architecture and proof-of-concept planning

---

## 1. Product Vision

Aeri is a modern, open-source, native desktop browser designed to combine Chromium's website compatibility with a lightweight, responsive, and polished user interface.

The product draws inspiration from Arc's workspace-oriented browsing experience, Chrome's profile separation, and the native responsiveness of applications built with GPUI.

Aeri must feel like a first-class macOS application rather than a web-based desktop wrapper.

The browser should eventually support Windows while preserving the same product identity and core functionality.

### Product Principles

1. **Native by design:** Use GPUI for browser chrome, application controls, and workspace management.
2. **Performance-conscious:** Optimize startup latency, responsiveness, memory usage, and background resource consumption.
3. **Profiles are first-class:** Provide Chrome-like profile isolation with independent browsing environments.
4. **Workspace-oriented:** Organize browsing into spaces, pinned tabs, and persistent sessions.
5. **Local-first:** Do not require accounts, subscriptions, or cloud services for core functionality.
6. **Open-source:** Maintain transparent architecture, documented APIs, and reproducible development workflows.
7. **Secure by default:** Preserve Chromium's security boundaries and implement robust permission handling.
8. **Maintainable:** Separate the browser engine, application logic, platform integrations, and UI.

### Product Positioning

Aeri is not intended to be another Electron browser shell.

It is a native desktop application with a dedicated browser engine integration.

The goal is to offer the visual polish of Arc, the familiar profile management of Chrome, and a fast, efficient application experience.

Performance improvements must be demonstrated through benchmarks rather than assumed from the choice of Rust or GPUI.

---

## 2. Technology Architecture

### 2.1 Proposed Stack

| Component | Technology |
| --- | --- |
| Application core | Rust |
| Native interface | GPUI |
| Browser engine | Chromium Embedded Framework |
| Browser integration | Rust FFI with CEF |
| macOS platform bridge | Swift / Objective-C++ where necessary |
| Windows platform bridge | Win32 / Windows APIs |
| Persistent metadata | SQLite |
| Password and secret storage | OS credential storage |
| Configuration | Versioned local configuration |
| Build tooling | Cargo, CMake, Ninja as needed |
| CI/CD | GitHub Actions |
| Distribution | Signed macOS application initially |

The agent must validate the compatibility of GPUI and CEF before finalizing the implementation strategy.

### 2.2 Architectural Boundaries

Aeri should have four primary architectural layers.

#### Layer A — Native Interface

Responsibilities:

- Application windows
- Sidebar and navigation
- Address bar
- Tab interface
- Profile switching
- Space switching
- Command palette
- Native menus
- Keyboard shortcuts
- Animation and transitions
- Visual themes

This layer should use GPUI wherever technically practical.

#### Layer B — Browser Application Core

Responsibilities:

- Browser lifecycle
- Navigation commands
- Tab state management
- Profile management
- Space management
- Session persistence
- History and bookmarks
- Permission coordination
- Download management
- Application settings

This layer must remain independent of the concrete UI framework wherever possible.

#### Layer C — Chromium Integration

Responsibilities:

- Browser instance creation and destruction
- Website rendering
- Navigation events
- Request contexts
- Cookie and cache isolation
- Browser process management
- Download events
- Permission requests
- Rendering surface integration
- Input and focus forwarding
- Browser crash detection

CEF-specific types must not leak throughout the application.

Define a dedicated browser-engine abstraction.

#### Layer D — Platform Integration

Responsibilities:

- Native window management
- macOS menus
- macOS Keychain
- File dialogs
- Notifications
- System appearance
- Trackpad and keyboard integration
- Window restoration
- Code signing and application packaging

Use platform-specific adapters rather than embedding operating-system logic directly into the browser core.

---

## 3. Critical Technical Validation

Before implementing the complete browser, the agent must investigate and validate GPUI + CEF integration.

This is the highest-priority engineering task.

### 3.1 Integration Approaches

Evaluate two approaches.

**Option A: Native CEF window embedding**

Embed a CEF-managed native browser view within the application window.

Investigate:

- GPUI window handle accessibility
- macOS NSView interoperability
- Native view hierarchy
- Resizing and layout synchronization
- Focus management
- GPU-accelerated Chromium rendering
- Overlay behavior
- Window transparency
- Multiple browser views

**Option B: CEF off-screen rendering**

Render Chromium content into an application-managed surface.

Investigate:

- CEF off-screen rendering APIs
- GPUI texture integration
- Pixel buffer transfer overhead
- GPU acceleration availability
- Input event translation
- Scroll performance
- Text selection
- IME support
- Popup rendering
- Rendering synchronization

Do not assume that off-screen rendering provides a zero-copy GPU pipeline.

### 3.2 Decision Criteria

Compare both approaches using:

- Implementation feasibility
- Rendering latency
- Memory overhead
- CPU utilization
- GPU utilization
- Scrolling smoothness
- Keyboard and IME correctness
- Native window integration
- Multi-tab scalability
- Long-term maintainability
- Windows portability

Select the approach supported by the strongest technical evidence.

If GPUI and CEF cannot be integrated reliably, document the blockers and propose alternatives without silently replacing the agreed stack.

### 3.3 Required Proof of Concept

The first working application must:

1. Launch a native macOS window.
2. Render its browser chrome using GPUI.
3. Embed a Chromium browser instance.
4. Navigate to a real HTTPS website.
5. Support keyboard input and scrolling.
6. Support resizing without visual corruption.
7. Handle navigation errors.
8. Close cleanly without orphaned subprocesses.

Also test text selection, copy/paste, IME input, popups, context menus, and focus transitions.

The proof of concept is complete only when the browser is demonstrably functional and the integration approach is documented.

---

## 4. Profiles Architecture

Profiles are a foundational feature of Aeri.

They must behave similarly to Chrome's independent browsing profiles.

### 4.1 Profile Model

Each profile owns:

- Unique identifier
- Display name
- Avatar or icon
- Theme/accent color
- Browser storage context
- Cookies
- HTTP cache
- Local storage
- IndexedDB
- Browsing history
- Bookmarks
- Saved credentials
- Permission settings
- Extension state when supported
- Spaces
- Tabs
- Session restoration state

Profiles must not accidentally share browser storage or application metadata.

### 4.2 Profile Isolation

Use dedicated CEF request contexts and persistent storage paths for each profile.

Example:

```
Aeri/
└── Profiles/
    ├── work/
    │   ├── chromium/
    │   ├── history.db
    │   ├── bookmarks.db
    │   └── session.db
    ├── personal/
    │   ├── chromium/
    │   ├── history.db
    │   ├── bookmarks.db
    │   └── session.db
    └── development/
        ├── chromium/
        ├── history.db
        ├── bookmarks.db
        └── session.db
```

The final directory structure should follow platform conventions.

Do not use profile display names as persistent identifiers.

Use stable generated identifiers.

Credential storage must use an appropriate OS-protected mechanism rather than plaintext database records.

### 4.3 Profile Features

Required:

- Create profile
- Rename profile
- Delete profile with confirmation
- Switch profile
- Open profile in a separate window
- Persist profile state
- Restore profile sessions
- Distinguish profiles visually
- Open a URL in another profile

Future:

- Profile import/export
- Temporary profiles
- Profile locking
- Optional encrypted synchronization

### 4.4 Profile Security

Aeri must isolate application-level state as well as Chromium-managed storage.

Separate request contexts are not equivalent to operating-system sandbox isolation.

The architecture must preserve Chromium process security and avoid exposing profile data across boundaries.

---

## 5. Spaces and Tabs

Aeri uses the following hierarchy:

```
Application
└── Profiles
    └── Spaces
        └── Tabs
```

A profile may contain multiple spaces.

Spaces share the browsing identity of their parent profile.

### 5.1 Spaces

Each space contains:

- Identifier
- Name
- Icon
- Accent color
- Ordered tabs
- Pinned tabs
- Active tab
- Session restoration metadata

Required operations:

- Create space
- Rename space
- Delete space
- Switch space
- Move tab between spaces
- Restore spaces after restart

### 5.2 Tabs

Required:

- Open tab
- Close tab
- Duplicate tab
- Reload tab
- Navigate backward/forward
- Pin tab
- Unpin tab
- Reorder tabs
- Move tab between spaces
- Restore closed tab
- Display favicon and title
- Display loading state
- Display audio activity where supported

### 5.3 Tab Lifecycle

Implement explicit tab states.

```
Created
   ↓
Loading
   ↓
Active
   ↓
Background
   ↓
Suspended
   ↓
Restored
```

Additional states must account for navigation failure, renderer crashes, and tab closure.

Suspension must not silently destroy important user state.

Investigate Chromium lifecycle APIs before implementing memory-saving behavior.

Avoid aggressive tab suspension in the initial release.

---

## 6. User Experience and Interface

Aeri should have a minimal, polished, native interface.

The design language should be inspired by high-quality macOS software without copying another browser's visual assets or distinctive interface wholesale.

### 6.1 Primary Layout

The default layout contains:

- Native application window
- Collapsible left sidebar
- Profile selector
- Space selector
- Pinned tabs
- Regular tabs
- Address/search bar
- Main website viewport

The interface should minimize unnecessary visual chrome.

### 6.2 Sidebar

The sidebar should support:

- Collapsing and expanding
- Smooth transitions
- Tab reordering
- Drag-and-drop
- Pinned tabs
- Profile identity
- Space switching
- Context menus
- Keyboard navigation

### 6.3 Command Palette

A keyboard-first command palette should eventually support:

- Search tabs
- Search history
- Open URL
- Search the web
- Switch profile
- Switch space
- Create tab
- Close tab
- Duplicate tab
- Move tab
- Execute browser commands

Suggested macOS shortcut:

`Cmd + L` for address/search.

Additional shortcuts should follow macOS conventions and remain configurable where appropriate.

### 6.4 Visual Quality

Prioritize:

- Consistent spacing
- High-quality typography
- Smooth resizing
- Appropriate native translucency
- Dark and light modes
- Accessible contrast
- Responsive input feedback
- Minimal visual clutter
- Reduced-motion support
- Correct high-DPI rendering

Do not sacrifice usability or performance for decorative effects.

---

## 7. Security and Privacy

Security is a release-blocking concern.

Aeri must not weaken Chromium security features to improve performance or simplify integration.

Requirements include:

- Preserve Chromium sandboxing
- Preserve site isolation where supported
- Validate navigation targets
- Handle TLS certificate errors safely
- Implement permission prompts
- Restrict privileged application interfaces
- Protect local credentials
- Avoid exposing Rust or native APIs to untrusted webpages
- Sanitize filenames and download destinations
- Maintain secure subprocess boundaries
- Handle renderer crashes safely
- Keep Chromium/CEF security updates current

Never silently bypass certificate warnings.

Never expose privileged application functions to arbitrary website JavaScript.

Profile data must remain isolated.

### Privacy Defaults

- No mandatory user account
- No application telemetry by default
- No advertising identifiers
- No browser activity collection
- No third-party analytics without explicit opt-in
- Local-first storage
- Clear browsing data controls

---

## 8. Performance Engineering

Performance must be measurable.

### 8.1 Metrics

Track:

- Cold startup time
- Warm startup time
- Time to interactive
- Tab creation latency
- Tab switching latency
- Navigation responsiveness
- Memory usage
- Idle CPU utilization
- GPU utilization
- Scrolling frame consistency
- Session restoration latency
- Browser subprocess count

### 8.2 Optimization Strategy

Investigate:

- Lazy browser instance creation
- Efficient tab state management
- Reduced unnecessary UI rerenders
- Asynchronous database operations
- Efficient event propagation
- Browser process reuse where appropriate
- Controlled background activity
- Session restoration batching
- Background tab suspension

Do not optimize before establishing baseline measurements.

Compare against mainstream Chromium browsers using equivalent workloads.

### 8.3 Benchmark Scenarios

At minimum:

- Empty browser window
- One active tab
- Ten tabs
- Thirty tabs
- Multiple profiles
- Multiple spaces
- Background tabs
- Heavy JavaScript website
- Video playback
- Browser restart with restored sessions

Record hardware, OS version, browser engine version, and test conditions.

---

## 9. Repository Architecture

Proposed monorepo:

```
aeri/
├── apps/
│   └── desktop/
│       ├── src/
│       └── Cargo.toml
│
├── crates/
│   ├── aeri-core/
│   ├── aeri-ui/
│   ├── aeri-engine/
│   ├── aeri-cef/
│   ├── aeri-profiles/
│   ├── aeri-spaces/
│   ├── aeri-tabs/
│   ├── aeri-storage/
│   ├── aeri-security/
│   └── aeri-platform/
│
├── platform/
│   ├── macos/
│   └── windows/
│
├── assets/
├── benchmarks/
├── tests/
├── scripts/
├── docs/
│   ├── architecture/
│   ├── decisions/
│   └── development/
│
├── .github/
│   └── workflows/
│
├── Cargo.toml
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
└── LICENSE
```

This is a target structure, not a requirement to create empty crates prematurely.

Start with the smallest architecture necessary for the proof of concept.

Split crates only when meaningful ownership boundaries emerge.

---

## 10. Development Phases

### Phase 0 — Research and Architecture

Deliverables:

- GPUI feasibility assessment
- CEF integration assessment
- macOS native-view integration research
- Rendering strategy comparison
- Browser engine abstraction proposal
- Dependency and licensing review
- Initial architecture decision records

Exit criteria: a technically defensible implementation strategy exists.

### Phase 1 — Browser Engine Proof of Concept

Deliverables:

- Working macOS application
- GPUI window
- Embedded Chromium
- URL navigation
- Keyboard and mouse interaction
- Scrolling
- Window resizing
- Basic browser lifecycle management
- Crash and shutdown handling

Exit criteria: a real website renders and behaves correctly inside the application.

### Phase 2 — Browser Foundation

Deliverables:

- Tab manager
- Navigation controls
- Address bar
- Browser history
- Bookmarks
- Downloads
- Basic permission handling
- Session persistence
- Error handling

Exit criteria: Aeri can perform basic everyday browsing tasks.

### Phase 3 — Profiles

Deliverables:

- Profile manager
- Dedicated request contexts
- Separate storage
- Profile-specific windows
- Profile switching
- Session restoration
- Profile isolation tests

Exit criteria: two profiles can open the same website with different login sessions without sharing cookies or other profile-specific data.

### Phase 4 — Spaces and Native Experience

Deliverables:

- Vertical sidebar
- Spaces
- Pinned tabs
- Command palette
- Keyboard shortcuts
- Drag-and-drop tabs
- Native macOS polish
- Theme support

Exit criteria: Aeri delivers its intended workspace-oriented browsing experience.

### Phase 5 — Optimization and Beta

Deliverables:

- Performance benchmarks
- Memory optimizations
- Crash recovery
- Accessibility improvements
- Automated tests
- Signed builds
- Secure update mechanism
- Security review

Exit criteria: Aeri is stable enough for a limited macOS beta.

### Phase 6 — Windows Support

Deliverables:

- Windows platform adapter
- Native Windows window integration
- Windows credential storage
- Windows packaging
- Windows CI
- Cross-platform integration tests

Exit criteria: the core browsing experience operates reliably on both platforms.

---

## 11. Testing Strategy

### Unit Tests

Cover:

- Profile lifecycle
- Tab state transitions
- Space management
- Session serialization
- Configuration validation
- Navigation state
- Storage migrations

### Integration Tests

Cover:

- Chromium lifecycle
- Request context isolation
- Navigation events
- Downloads
- Permission handling
- Profile switching
- Window closing
- Browser crashes
- Session restoration

### End-to-End Tests

Use local test websites to validate:

- Form submission
- Cookie persistence
- Local storage
- IndexedDB
- Multiple profile isolation
- Authentication sessions
- File uploads
- Downloads
- Keyboard input
- Popup windows
- Browser restart behavior

Avoid relying exclusively on external websites for automated tests.

---

## 12. Open-Source Strategy

Proposed license: Apache-2.0, subject to dependency and licensing review.

Required project files:

- README.md
- CONTRIBUTING.md
- CODE_OF_CONDUCT.md
- SECURITY.md
- LICENSE
- Third-party notices
- Development setup guide
- Architecture documentation

Document how to report security vulnerabilities privately.

Do not publish secrets, signing keys, or sensitive build credentials.

Automate dependency auditing and release checks.

---

## 13. Explicit Non-Goals for Initial Development

Do not implement the following during the initial proof of concept:

- AI assistants
- Cloud synchronization
- User accounts
- Mobile applications
- Custom browser rendering engine
- Chromium fork
- Custom extension marketplace
- Built-in VPN
- Advertising or monetization
- Complex tab suspension algorithms
- Multi-device synchronization
- Custom password manager
- Browser automation agents

Chrome extension compatibility is a potential future requirement, not a capability guaranteed by embedding CEF.

Investigate it separately before making product promises.

---

## 14. Agent Execution Instructions

You are acting as the principal software architect and founding engineer for Aeri Browser.

Your task is to turn this specification into an executable engineering plan and then implement the smallest viable technical proof of concept.

### Required Working Principles

1. Research the current GPUI and CEF APIs before writing integration code.
2. Verify dependency versions, supported platforms, and licensing.
3. Do not invent APIs or assume unsupported integration capabilities.
4. Prefer documented, maintainable interfaces over undocumented hacks.
5. Keep the Rust core independent from UI and platform implementations.
6. Use safe Rust wherever possible.
7. Isolate unsafe FFI code behind small, reviewed interfaces.
8. Handle errors explicitly.
9. Implement appropriate tracing and diagnostics.
10. Write tests for meaningful logic.
11. Avoid unnecessary dependencies and premature abstractions.
12. Do not weaken browser security to achieve performance targets.
13. Record important architectural decisions.
14. Prefer incremental, demonstrable milestones.

### First Assignment

Begin with **Phase 0: Architecture and Feasibility**.

Produce:

1. `docs/architecture/overview.md`
2. `docs/architecture/gpui-cef-integration.md`
3. `docs/architecture/profile-isolation.md`
4. `docs/decisions/0001-browser-engine.md`
5. `docs/decisions/0002-rendering-strategy.md`
6. `docs/roadmap.md`
7. `docs/development.md`

Then propose the minimal Phase 1 implementation.

Do not begin implementing the complete product before validating the browser engine integration.

After Phase 0, proceed to a focused proof of concept if the environment and integration strategy are sufficiently validated.

### Definition of Success

The first major success milestone is:

**Aeri launches as a native macOS application, uses GPUI for its browser interface, renders a real website through Chromium, supports essential input and navigation, and shuts down cleanly.**

The next major success milestone is:

**Aeri supports multiple independent profiles with separate browser storage, persistent sessions, and workspace organization.**

The long-term goal is a secure, polished, high-performance, open-source browser suitable for daily use.

---

**Project:** Aeri Browser
**Guiding Principle:** Native feel. Chromium compatibility. Independent browsing identities. Open by design.
