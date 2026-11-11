# gpui's Linux and wgpu backends, vendored

`gpui-pre-linux` and `gpui-pre-wgpu` 0.3.9, as crates.io publishes them,
taken in through `[patch.crates-io]` in the top `Cargo.toml`, with these
changes on top. The first commit that touches them adds them unchanged,
without their `Cargo.lock` and the wgpu backend's benchmark, so
`git log -p -- vendor` shows every change made since; in the code,
`gpui-greeter:` comments mark them. Both crates are Zed's, under the
Apache License 2.0, whose text is beside each.

- **A window dragged to a new size gets its frames** (`x11/client.rs`,
  `x11/window.rs`, `wgpu_renderer.rs`). NVIDIA's X11 Vulkan surfaces put a
  swapchain out of date at any change of their window's size, and making
  one takes longer than the pointer takes to change the size again, so a
  window that presents into itself got one to five frames a second to the
  screen while it was dragged larger, and its content lagged the pointer
  by up to half a second. Measured on NVIDIA with a probe, a fixed child
  window as large as the screen that the window clips took 210 to 235
  frames a second, 6 to 9 pixels behind the pointer.

  So the GPU presents into a child of the window rather than the window
  itself. While the size keeps changing (a size within a quarter of a
  second of the last), the child is as large as the screen, or half as
  large again as a window that outgrows the screen, and the window clips
  it, so its swapchain stays good; once the size has rested for a tenth
  of a second over two frames and the server has the size last seen, the
  child is the window's size again. Frames are presented at once while
  the child is larger than the window and at the vertical blank (FIFO) at
  rest. The child's exposures count as the window's, and the pointer
  moving into it stays in the window. Of the ConfigureNotify events read
  in one go, each window's last is handled, and the layout and the
  swapchain take its size without a round trip to the server, which
  would wait behind the window manager's resizes; the swapchain follows a
  new size at the next frame, so sizes that come and go in between cost
  none, and a frame for a swapchain a size behind is shown rather than
  dropped. The window keeps its last frame at its top left (north-west bit
  gravity) until the next is drawn, and the settings app gives the window
  and the child its ground as their background, so a strip a window gains
  shows the ground rather than black.

  The same change carries the window resizing of silent, a file previewer
  drawn with gpui, without silent's other changes to these crates. The
  greeter's own windows, over whole monitors and never resized, present
  into a child the size of the window, as before into the window itself.

- **Each window keeps to its own monitor's refresh rate** (`x11/refresh.rs`,
  `x11/client.rs`). gpui gives a window its frames on a timer at the rate
  of the first lit CRTC's mode, read once, when the window is first shown,
  and kept as long as the window lasts; with no CRTC lit then, 60 Hz. So a
  window on a second monitor ran at the first one's rate, and a window
  first shown while its monitor slept, as the lock can be (NVIDIA then
  lists the monitor as disconnected, with no modes), stayed at 60 Hz after
  the monitor woke.

  Now a window takes the current mode of the CRTC it lies over most, the
  faster of mirrored ones, read again each time it is shown, 100 ms after
  it last moved or changed size, and whenever RandR tells of a change to
  the screen, a CRTC or an output. A window shown again starts at the rate
  it had and reads it after its first frame, so that the frame waits for
  no round trip; one moved or resized reads it once it has rested, since
  a round trip while it is dragged would wait behind each move. silent's
  copy of these crates does the same. While no lit CRTC lies under it, a
  window keeps the rate it had from one; failing that it takes the
  fastest lit CRTC's, then the fastest mode RandR lists, then 60 Hz,
  until RandR tells of a monitor lit under it. Each window's rate, and
  what it comes from, is logged at info with the target
  `gpui_linux::linux::x11::refresh`, which `GPUI_GREETER_FRAMES=1` turns
  on.

- **Pop-ups stay composited** (`x11/window.rs`). The greeter's only
  pop-ups are the lock's windows, over whole monitors, and before they are
  mapped they ask a compositor never to unredirect them
  (`_NET_WM_BYPASS_COMPOSITOR` set to 2), for the reason
  [docs/lock-screen.md](../docs/lock-screen.md) gives.

To take a newer gpui, copy its crates over these, drop their `Cargo.lock`
and benchmarks, and carry the changes over.
