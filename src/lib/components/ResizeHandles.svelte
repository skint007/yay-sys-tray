<script lang="ts">
  // Invisible edge/corner handles that drive native resizing of the frameless
  // window. Guarded so the browser preview (no Tauri) is a no-op.
  function resize(dir: string) {
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) => getCurrentWindow().startResizeDragging(dir as never))
      .catch(() => {});
  }
</script>

<div class="handles">
  <div class="edge n" onmousedown={() => resize("North")} role="none"></div>
  <div class="edge s" onmousedown={() => resize("South")} role="none"></div>
  <div class="edge e" onmousedown={() => resize("East")} role="none"></div>
  <div class="edge w" onmousedown={() => resize("West")} role="none"></div>
  <div class="corner nw" onmousedown={() => resize("NorthWest")} role="none"></div>
  <div class="corner ne" onmousedown={() => resize("NorthEast")} role="none"></div>
  <div class="corner sw" onmousedown={() => resize("SouthWest")} role="none"></div>
  <div class="corner se" onmousedown={() => resize("SouthEast")} role="none"></div>
</div>

<style>
  .handles { position: fixed; inset: 0; z-index: 9999; pointer-events: none; }
  .edge, .corner { position: absolute; pointer-events: auto; }
  .edge.n { top: 0; left: 8px; right: 8px; height: 4px; cursor: ns-resize; }
  .edge.s { bottom: 0; left: 8px; right: 8px; height: 4px; cursor: ns-resize; }
  .edge.e { right: 0; top: 8px; bottom: 8px; width: 4px; cursor: ew-resize; }
  .edge.w { left: 0; top: 8px; bottom: 8px; width: 4px; cursor: ew-resize; }
  .corner { width: 12px; height: 12px; }
  .corner.nw { top: 0; left: 0; cursor: nwse-resize; }
  .corner.ne { top: 0; right: 0; cursor: nesw-resize; }
  .corner.sw { bottom: 0; left: 0; cursor: nesw-resize; }
  .corner.se { bottom: 0; right: 0; cursor: nwse-resize; }
</style>
