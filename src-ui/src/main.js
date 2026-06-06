import { invoke } from '@tauri-apps/api/core';
import './styles.css';
const app = document.getElementById('app');
let selectedSlotId = null;
let mockSurfaceId = null;
let activePageId = null;
app.innerHTML = `
  <header>
    <h1>integratedeck</h1>
    <span class="status" id="status">Starting…</span>
    <button id="btn-save">Save profile</button>
    <button id="btn-mock-surface">Add mock surface</button>
  </header>
  <aside>
    <div class="section">
      <h2>Stream Deck plugins</h2>
      <ul class="plugins" id="sd-plugins"></ul>
      <button id="btn-scan">Scan plugins</button>
    </div>
    <div class="section">
      <h2>Companion modules</h2>
      <ul class="plugins" id="companion-modules"></ul>
    </div>
    <div class="section">
      <h2>Connections</h2>
      <ul class="plugins" id="connections"></ul>
      <button id="btn-add-connection">Add test connection</button>
    </div>
    <p class="warn">Close the Elgato Stream Deck app when using physical Stream Deck hardware.</p>
  </aside>
  <main>
    <div class="grid" id="grid"></div>
    <button id="btn-trigger">Trigger selected slot</button>
  </main>
  <div class="inspector">
    <div class="section">
      <h2>Property Inspector</h2>
      <p class="status" id="pi-status">Select a slot with an SD binding</p>
      <iframe id="pi-frame" title="Property Inspector" sandbox="allow-scripts allow-same-origin"></iframe>
    </div>
    <div class="section">
      <h2>Bind SD action</h2>
      <label>Plugin UUID <input id="plugin-uuid" type="text" style="width:100%" /></label>
      <label>Action UUID <input id="action-uuid" type="text" style="width:100%" /></label>
      <button id="btn-bind">Bind to selected slot</button>
    </div>
  </div>
`;
const statusEl = document.getElementById('status');
const gridEl = document.getElementById('grid');
async function refresh() {
    const info = await invoke('get_app_info');
    statusEl.textContent = `v${info.version} · ${info.data_dir}`;
    const profile = await invoke('get_profile');
    activePageId = profile.active_page_id ?? profile.pages[0]?.id ?? null;
    if (!mockSurfaceId && profile.surfaces.length === 0) {
        mockSurfaceId = await invoke('register_mock_surface', { name: 'Mock 3×5' });
        await refresh();
        return;
    }
    if (profile.surfaces.length > 0) {
        mockSurfaceId = profile.surfaces[0].surface_id;
    }
    renderGrid(profile);
    await loadPlugins();
    await loadConnections();
}
function slotUuid(slot) {
    return slot.id;
}
function renderGrid(profile) {
    gridEl.innerHTML = '';
    const page = profile.pages.find((p) => p.id === activePageId) ?? profile.pages[0];
    if (!page)
        return;
    for (let row = 0; row < 3; row++) {
        for (let col = 0; col < 5; col++) {
            const slot = Object.values(page.slots).find((s) => s.locator.row === row && s.locator.column === col);
            const el = document.createElement('button');
            el.className = 'slot' + (slot && slotUuid(slot) === selectedSlotId ? ' selected' : '');
            el.type = 'button';
            el.dataset.row = String(row);
            el.dataset.col = String(col);
            el.textContent = slot ? `${row},${col}` : '+';
            el.addEventListener('click', () => onSlotClick(page, row, col, slot));
            gridEl.appendChild(el);
        }
    }
}
async function onSlotClick(page, row, col, existing) {
    if (!mockSurfaceId || !activePageId)
        return;
    let slot = existing;
    if (!slot) {
        slot = await invoke('create_slot', {
            surfaceId: mockSurfaceId,
            pageId: activePageId,
            row,
            column: col,
        });
        await refresh();
        slot = Object.values(page.slots).find((s) => s.locator.row === row && s.locator.column === col) ?? slot;
    }
    selectedSlotId = slotUuid(slot);
    renderGrid(await invoke('get_profile'));
}
async function loadPlugins() {
    const scan = await invoke('scan_plugins');
    const sdList = document.getElementById('sd-plugins');
    sdList.innerHTML = scan.streamdeck
        .map((p) => {
        const name = p.split('/').pop() ?? p;
        return `<li data-path="${encodeURIComponent(p)}">${name}</li>`;
    })
        .join('') || '<li>No plugins found</li>';
    sdList.querySelectorAll('li[data-path]').forEach((li) => {
        li.addEventListener('click', async () => {
            const path = decodeURIComponent(li.dataset.path);
            const loaded = await invoke('load_sd_plugin', { path });
            statusEl.textContent = `Loaded ${loaded.name} on port ${loaded.port}`;
            document.getElementById('plugin-uuid').value = loaded.plugin_uuid;
        });
    });
    const compList = document.getElementById('companion-modules');
    compList.innerHTML = scan.companion
        .map((p) => `<li>${p.split('/').pop() ?? p}</li>`)
        .join('') || '<li>No modules found</li>';
}
async function loadConnections() {
    const list = await invoke('list_connections');
    const el = document.getElementById('connections');
    el.innerHTML = list.map((c) => `<li>${c.label} (${c.module_id})</li>`).join('') || '<li>None</li>';
}
document.getElementById('btn-scan').addEventListener('click', () => loadPlugins());
document.getElementById('btn-save').addEventListener('click', async () => {
    await invoke('save_profile');
    statusEl.textContent = 'Profile saved';
});
document.getElementById('btn-mock-surface').addEventListener('click', async () => {
    mockSurfaceId = await invoke('register_mock_surface', { name: 'Mock surface' });
    await refresh();
});
document.getElementById('btn-bind').addEventListener('click', async () => {
    if (!selectedSlotId)
        return;
    const pluginUuid = document.getElementById('plugin-uuid').value;
    const actionUuid = document.getElementById('action-uuid').value;
    await invoke('bind_slot_sd', {
        slotId: selectedSlotId,
        pluginUuid,
        actionUuid,
    });
    statusEl.textContent = 'Bound SD action';
});
document.getElementById('btn-trigger').addEventListener('click', async () => {
    if (!selectedSlotId)
        return;
    await invoke('trigger_slot', { slotId: selectedSlotId });
});
document.getElementById('btn-add-connection').addEventListener('click', async () => {
    await invoke('add_connection', {
        moduleId: 'companion-module-generic-test',
        label: 'Test connection',
        config: {},
    });
    await loadConnections();
    const ping = await invoke('sidecar_ping');
    if (ping)
        statusEl.textContent = `Sidecar ping sent: ${ping}`;
});
refresh().catch((e) => {
    statusEl.textContent = String(e);
});
