/* WinLayout 前端：可视化布局编辑器
 * 通过 withGlobalTauri 注入的 window.__TAURI__.core.invoke 与 Rust 后端通信。
 */
(() => {
  'use strict';

  const T = window.__TAURI__;
  // 前端异常上报到后端日志，方便排查 WebView 运行时问题
  window.addEventListener('error', (e) => {
    if (T && T.core && T.core.invoke) {
      T.core.invoke('frontend_log', { msg: 'JS 错误: ' + e.message + ' @' + (e.filename || '') + ':' + (e.lineno || 0) }).catch(() => {});
    }
  });
  const invoke = (cmd, args = {}) => {
    if (!T || !T.core || !T.core.invoke) {
      throw new Error('Tauri 运行时未就绪（请用 winlayout.exe 打开）');
    }
    return T.core.invoke(cmd, args);
  };

  const $ = (id) => document.getElementById(id);
  const uid = () => 'id' + Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4);
  const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
  const pct = (v) => (v * 100).toFixed(1);

  let S = { settings: {}, layouts: [], work_area: { x: 0, y: 0, w: 1920, h: 1080 }, monitors: [], autostart: false };
  let curName = null;
  let selTile = null;
  let selLayer = null;
  let drag = null;
  let saveTimer = null;
  let ctxMenu = null;

  /* ---------------- 通用 UI ---------------- */
  function toast(msg, err = false) {
    document.querySelectorAll('.toast').forEach((n) => n.remove());
    const el = document.createElement('div');
    el.className = 'toast' + (err ? ' err' : '');
    el.textContent = msg;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), err ? 5200 : 2600);
  }
  function status(msg) { $('status-text').textContent = msg; }
  function curLayout() { return S.layouts.find((l) => l.name === curName) || null; }
  function curTile() {
    const l = curLayout();
    return l ? l.tiles.find((t) => t.id === selTile) || null : null;
  }
  function curLayer() {
    const t = curTile();
    return t ? t.layers.find((l) => l.id === selLayer) || null : null;
  }

  function openModal(title, bodyHtml, buttons, wide = false) {
    $('modal-title').textContent = title;
    $('modal-body').innerHTML = bodyHtml;
    $('modal').classList.toggle('wide', wide);
    const foot = $('modal-foot');
    foot.innerHTML = '';
    buttons.forEach((b) => {
      const btn = document.createElement('button');
      btn.className = 'btn' + (b.primary ? ' primary' : '') + (b.danger ? ' danger-ghost' : '');
      btn.textContent = b.label;
      btn.onclick = async () => {
        if (!b.action) return closeModal();
        const r = await b.action();
        if (r !== false) closeModal();
      };
      foot.appendChild(btn);
    });
    $('modal-backdrop').classList.remove('hidden');
  }
  function closeModal() { $('modal-backdrop').classList.add('hidden'); }
  $('modal-x').onclick = closeModal;
  $('modal-backdrop').addEventListener('mousedown', (e) => { if (e.target === $('modal-backdrop')) closeModal(); });

  function promptOne(title, label, value, onOk, desc = '') {
    openModal(title,
      `${desc ? `<p class="desc">${desc}</p>` : ''}<div class="form-row"><label>${label}</label><input id="m-input" value="${escapeAttr(value)}" /></div>`,
      [{ label: '取消' }, {
        label: '确定', primary: true, action: () => { const v = $('m-input').value.trim(); if (!v) { toast('名称不能为空', true); return false; } return onOk(v); }
      }]);
    setTimeout(() => { const i = $('m-input'); if (i) { i.focus(); i.select(); } }, 30);
  }
  function confirmBox(title, text, onOk) {
    openModal(title, `<p class="desc">${text}</p>`,
      [{ label: '取消' }, { label: '确定', primary: true, danger: true, action: onOk }]);
  }
  function escapeHtml(s) {
    return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  }
  function escapeAttr(s) { return escapeHtml(s).replace(/`/g, '&#96;'); }

  function showContextMenu(x, y, items) {
    hideContextMenu();
    ctxMenu = document.createElement('div');
    ctxMenu.className = 'ctx-menu';
    ctxMenu.style.left = Math.min(x, window.innerWidth - 170) + 'px';
    ctxMenu.style.top = Math.min(y, window.innerHeight - 220) + 'px';
    items.forEach((it) => {
      if (it === '-') { const hr = document.createElement('hr'); ctxMenu.appendChild(hr); return; }
      const d = document.createElement('div');
      d.textContent = it.label;
      d.onclick = () => { hideContextMenu(); it.action(); };
      ctxMenu.appendChild(d);
    });
    document.body.appendChild(ctxMenu);
  }
  function hideContextMenu() { if (ctxMenu) { ctxMenu.remove(); ctxMenu = null; } }
  document.addEventListener('mousedown', (e) => { if (ctxMenu && !ctxMenu.contains(e.target)) hideContextMenu(); });

  /* ---------------- 数据 ---------------- */
  async function load(keepSelection = false) {
    try {
      const st = await invoke('get_state');
      const old = curName;
      S = st;
      if (!keepSelection || !S.layouts.some((l) => l.name === old)) {
        curName = S.layouts.length ? S.layouts[0].name : null;
      }
      renderAll();
      syncActive();
      status('已载入 ' + S.layouts.length + ' 个布局');
      invoke('frontend_log', { msg: `UI 就绪：${S.layouts.length} 个布局 / 当前 ${curLayout() ? curLayout().tiles.length : 0} 个分区` }).catch(() => {});
      if (S.auto_detect) {
        invoke('detect_hotkeys')
          .then((r) => invoke('frontend_log', { msg: `自动检测完成：可用 ${r.ok_count} / 被占用 ${r.occupied_count} / 候选 ${r.candidates.length}` }))
          .catch((e) => invoke('frontend_log', { msg: '自动检测失败：' + e }));
      }
    } catch (e) {
      toast('载入失败: ' + e, true);
    }
  }
  async function save(showMsg = false) {
    if (!S.layouts) return;
    try {
      await invoke('save_state', { settings: S.settings, layouts: S.layouts });
      if (showMsg) { toast('已保存'); status('布局已保存'); }
    } catch (e) { toast('保存失败: ' + e, true); }
  }
  function scheduleSave() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => save(false), 450);
  }
  // 让「Ctrl 拖动窗口落入分区」以当前选中的布局为准
  function syncActive() {
    if (curName) invoke('set_active_layout', { name: curName }).catch(() => {});
  }

  function renderAll() {
    renderLayoutList();
    renderCanvas();
    renderProps();
    const wa = S.work_area || { w: 0, h: 0 };
    $('canvas-info').textContent = `工作区 ${wa.w}×${wa.h} · ${(S.monitors || []).length || 1} 屏`;
    $('status-workarea').textContent = `工作区 ${wa.x},${wa.y} ${wa.w}×${wa.h}`;
    $('chk-snap').checked = !!S.settings.snap_enabled;
  }

  function renderLayoutList() {
    const box = $('layout-list');
    box.innerHTML = '';
    $('layout-count').textContent = S.layouts.length;
    S.layouts.forEach((l, i) => {
      const nApp = l.tiles.reduce((a, t) => a + t.layers.length, 0);
      const el = document.createElement('div');
      el.className = 'layout-item' + (l.name === curName ? ' active' : '');
      el.innerHTML = `<div class="li-name">${i + 1}. ${escapeHtml(l.name)}</div>
        <div class="li-meta">${l.tiles.length} 分区 · ${nApp} 个应用</div>`;
      el.onclick = () => { curName = l.name; selTile = null; selLayer = null; renderAll(); syncActive(); status('当前布局：' + l.name); };
      el.ondblclick = () => { applyLayout(); };
      box.appendChild(el);
    });
  }

  /* ---------------- 画布 ---------------- */
  function fitCanvas() {
    const wrap = $('canvas-wrap');
    const wa = S.work_area || { w: 1920, h: 1080 };
    const maxW = Math.max(200, wrap.clientWidth - 10);
    const maxH = Math.max(150, wrap.clientHeight - 10);
    const scale = Math.min(maxW / wa.w, maxH / wa.h);
    const c = $('canvas');
    c.style.width = Math.floor(wa.w * scale) + 'px';
    c.style.height = Math.floor(wa.h * scale) + 'px';
  }
  window.addEventListener('resize', () => { fitCanvas(); renderCanvas(); });

  function renderCanvas() {
    const L = curLayout();
    const host = $('tiles');
    host.innerHTML = '';
    fitCanvas();
    $('canvas-empty').classList.toggle('hidden', !!L && L.tiles.length > 0);
    if (!L) return;
    const cw = $('canvas').clientWidth || 800;
    const ch = $('canvas').clientHeight || 450;
    L.tiles.forEach((t, ti) => {
      const el = document.createElement('div');
      el.className = 'tile' + (t.id === selTile ? ' selected' : '');
      el.dataset.tid = t.id;
      el.style.left = (t.rect[0] * 100) + '%';
      el.style.top = (t.rect[1] * 100) + '%';
      el.style.width = (t.rect[2] * 100) + '%';
      el.style.height = (t.rect[3] * 100) + '%';
      el.style.zIndex = String(10 + ti);
      const tag = document.createElement('div');
      tag.className = 'tag';
      tag.textContent = `#${ti + 1}${t.layers.length > 1 ? ' · ' + t.layers.length + ' 层' : ''}`;
      el.appendChild(tag);

      // 多层重叠：按 Z 序画成层叠卡片
      const n = t.layers.length;
      const tw = t.rect[2] * cw, th = t.rect[3] * ch;
      const inset = 16;
      const stepX = clamp((t.cascade[0] || 0) * (cw / (S.work_area.w || 1920)), 0, 14);
      const stepY = clamp((t.cascade[1] || 0) * (ch / (S.work_area.h || 1080)), 0, 14);
      const ox = n > 1 ? Math.max(stepX, 5) : 0;
      const oy = n > 1 ? Math.max(stepY, 5) : 0;
      t.layers.forEach((ly, li) => {
        const card = document.createElement('div');
        card.className = 'layer-card' + (ly.id === selLayer ? ' active' : '');
        card.dataset.lid = ly.id;
        card.dataset.tid = t.id;
        const off = li * ox;
        const offY = li * oy;
        card.style.left = (inset + off) + 'px';
        card.style.top = (inset + 22 + offY) + 'px';
        card.style.width = Math.max(30, tw - inset * 2 - off - 4) + 'px';
        card.style.height = Math.max(18, Math.min(26, th - inset - 26 - offY - 4)) + 'px';
        card.style.zIndex = String(li + 1);
        card.innerHTML = `<span class="lc-idx">${li + 1}</span><span class="lc-name">${escapeHtml(ly.name || ly.cmd || '应用')}</span>`;
        el.appendChild(card);
      });
      // 8 个缩放手柄
      [['nw', 0, 0], ['n', 50, 0], ['ne', 100, 0], ['e', 100, 50], ['se', 100, 100],
       ['s', 50, 100], ['sw', 0, 100], ['w', 0, 50]].forEach(([dir, px, py]) => {
        const h = document.createElement('div');
        h.className = 'tile-handle';
        h.dataset.dir = dir;
        h.style.left = `calc(${px}% - 4px)`;
        h.style.top = `calc(${py}% - 4px)`;
        h.style.cursor = dir + '-resize';
        el.appendChild(h);
      });
      host.appendChild(el);
    });
  }

  function drawGuides(gx, gy) {
    const g = $('guides');
    g.innerHTML = '';
    if (gx != null) { const d = document.createElement('div'); d.className = 'guide v'; d.style.left = (gx * 100) + '%'; g.appendChild(d); }
    if (gy != null) { const d = document.createElement('div'); d.className = 'guide h'; d.style.top = (gy * 100) + '%'; g.appendChild(d); }
  }

  function snapValue(v, candidates, tol) {
    let best = null, dist = tol;
    for (const c of candidates) {
      const d = Math.abs(v - c);
      if (d <= dist) { dist = d; best = c; }
    }
    return best;
  }
  function snapCandidates(L, tid, axis) {
    const list = [0, 0.5, 1];
    L.tiles.forEach((t) => {
      if (t.id === tid) return;
      if (axis === 'x') { list.push(t.rect[0], t.rect[0] + t.rect[2]); }
      else { list.push(t.rect[1], t.rect[1] + t.rect[3]); }
    });
    return list;
  }

  function linkNeighbours(L, tid, orig, next) {
    if (!$('chk-link').checked) return;
    const eps = 0.004;
    const dL = next[0] - orig[0];
    const dR = (next[0] + next[2]) - (orig[0] + orig[2]);
    const dT = next[1] - orig[1];
    const dB = (next[1] + next[3]) - (orig[1] + orig[3]);
    L.tiles.forEach((t) => {
      if (t.id === tid) return;
      const r = t.rect;
      if (Math.abs(dL) > eps && Math.abs(r[0] + r[2] - orig[0]) < eps) {
        const right = r[0] + r[2] + dL;   // 右边界跟着左边界走
        r[2] = Math.max(0.03, right - r[0]);
      }
      if (Math.abs(dR) > eps && Math.abs(r[0] - (orig[0] + orig[2])) < eps) {
        const right = r[0] + r[2];
        r[0] = clamp(r[0] + dR, 0, 1);
        r[2] = Math.max(0.03, right - r[0]);
      }
      if (Math.abs(dT) > eps && Math.abs(r[1] + r[3] - orig[1]) < eps) {
        const bottom = r[1] + r[3] + dT;
        r[3] = Math.max(0.03, bottom - r[1]);
      }
      if (Math.abs(dB) > eps && Math.abs(r[1] - (orig[1] + orig[3])) < eps) {
        const bottom = r[1] + r[3];
        r[1] = clamp(r[1] + dB, 0, 1);
        r[3] = Math.max(0.03, bottom - r[1]);
      }
    });
  }

  $('tiles').addEventListener('mousedown', (e) => {
    const L = curLayout();
    if (!L) return;
    const tileEl = e.target.closest('.tile');
    if (!tileEl) {
      // 双击空白新建分区
      return;
    }
    const tid = tileEl.dataset.tid;
    const t = L.tiles.find((x) => x.id === tid);
    if (!t) return;
    const layerEl = e.target.closest('.layer-card');
    if (layerEl) {
      selTile = tid; selLayer = layerEl.dataset.lid;
      renderCanvas(); renderProps();
      return;
    }
    selTile = tid;
    if (!t.layers.some((l) => l.id === selLayer)) selLayer = t.layers.length ? t.layers[t.layers.length - 1].id : null;
    const handle = e.target.closest('.tile-handle');
    let mode = 'move';
    if (handle) mode = handle.dataset.dir;
    else {
      const r = tileEl.getBoundingClientRect();
      const edge = 7;
      const left = e.clientX - r.left < edge, right = r.right - e.clientX < edge;
      const top = e.clientY - r.top < edge, bottom = r.bottom - e.clientY < edge;
      if (left || right || top || bottom) mode = (top ? 'n' : '') + (bottom ? 's' : '') + (left ? 'w' : '') + (right ? 'e' : '') || 'move';
    }
    drag = {
      tid, mode,
      sx: e.clientX, sy: e.clientY,
      orig: [...t.rect],
      snapshot: L.tiles.map((x) => ({ id: x.id, rect: [...x.rect] })),
      moved: false
    };
    renderCanvas(); renderProps();
    e.preventDefault();
  });

  window.addEventListener('mousemove', (e) => {
    if (!drag) return;
    const L = curLayout(); if (!L) return;
    const t = L.tiles.find((x) => x.id === drag.tid); if (!t) return;
    const cw = $('canvas').clientWidth, ch = $('canvas').clientHeight;
    const dx = (e.clientX - drag.sx) / cw, dy = (e.clientY - drag.sy) / ch;
    if (Math.abs(e.clientX - drag.sx) + Math.abs(e.clientY - drag.sy) > 2) drag.moved = true;
    const o = drag.orig;
    let r = [...o];
    const m = drag.mode;
    if (m === 'move') { r[0] = o[0] + dx; r[1] = o[1] + dy; }
    else {
      if (m.includes('w')) { r[0] = o[0] + dx; r[2] = o[2] - dx; }
      if (m.includes('e')) { r[2] = o[2] + dx; }
      if (m.includes('n')) { r[1] = o[1] + dy; r[3] = o[3] - dy; }
      if (m.includes('s')) { r[3] = o[3] + dy; }
    }
    // 最小尺寸与边界
    if (r[2] < 0.03) { if (m.includes('w')) r[0] = o[0] + o[2] - 0.03; r[2] = 0.03; }
    if (r[3] < 0.03) { if (m.includes('n')) r[1] = o[1] + o[3] - 0.03; r[3] = 0.03; }
    r[0] = clamp(r[0], 0, 1 - r[2]);
    r[1] = clamp(r[1], 0, 1 - r[3]);
    // 贴边吸附
    let gx = null, gy = null;
    if ($('chk-snap').checked && S.settings.snap_enabled !== false) {
      const tol = (S.settings.snap_px || 10) / cw;
      const tolY = (S.settings.snap_px || 10) / ch;
      const cx = snapCandidates(L, drag.tid, 'x'), cy = snapCandidates(L, drag.tid, 'y');
      const moveLeft = m === 'move' || m.includes('w');
      const moveRight = m === 'move' || m.includes('e');
      const moveTop = m === 'move' || m.includes('n');
      const moveBottom = m === 'move' || m.includes('s');
      if (moveLeft) { const s = snapValue(r[0], cx, tol); if (s != null) { r[2] += r[0] - s; r[0] = s; gx = s; } }
      if (moveRight) { const s = snapValue(r[0] + r[2], cx, tol); if (s != null) { r[2] = s - r[0]; gx = s; } }
      if (moveTop) { const s = snapValue(r[1], cy, tolY); if (s != null) { r[3] += r[1] - s; r[1] = s; gy = s; } }
      if (moveBottom) { const s = snapValue(r[1] + r[3], cy, tolY); if (s != null) { r[3] = s - r[1]; gy = s; } }
    }
    t.rect = r.map((v) => Math.round(v * 10000) / 10000);
    if (m !== 'move') {
      // 用快照恢复其它分区，再按边界联动，避免累积误差
      L.tiles.forEach((x) => { if (x.id !== drag.tid) { const snap = drag.snapshot.find((s) => s.id === x.id); if (snap) x.rect = [...snap.rect]; } });
      linkNeighbours(L, drag.tid, drag.orig, t.rect);
    }
    drawGuides(gx, gy);
    renderCanvas();
    renderPropsValues();
  });

  window.addEventListener('mouseup', () => {
    if (!drag) return;
    const moved = drag.moved;
    drag = null;
    drawGuides(null, null);
    if (moved) { scheduleSave(); status('已调整分区（自动保存）'); }
    renderCanvas();
  });

  $('canvas').addEventListener('dblclick', (e) => {
    const L = curLayout(); if (!L) return;
    if (e.target.closest('.tile')) return;
    const box = $('canvas').getBoundingClientRect();
    const x = clamp((e.clientX - box.left) / box.width - 0.16, 0, 0.68);
    const y = clamp((e.clientY - box.top) / box.height - 0.2, 0, 0.6);
    addTile([x, y, 0.32, 0.4]);
  });
  $('canvas').addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const L = curLayout(); if (!L) return;
    const tileEl = e.target.closest('.tile');
    if (tileEl) { selTile = tileEl.dataset.tid; renderCanvas(); renderProps(); }
    showContextMenu(e.clientX, e.clientY, [
      { label: '纵向拆分选中分区', action: () => splitTile('v') },
      { label: '横向拆分选中分区', action: () => splitTile('h') },
      { label: '从此分区添加应用', action: () => pickWindow() },
      '-',
      { label: '删除选中分区', action: () => deleteTile() },
      { label: '选中分区置顶', action: () => raiseTile() }
    ]);
  });

  function addTile(rect) {
    const L = curLayout(); if (!L) return;
    const t = { id: uid(), rect, cascade: [24, 24], layers: [] };
    L.tiles.push(t);
    selTile = t.id; selLayer = null;
    renderAll(); scheduleSave();
  }
  function splitTile(dir) {
    const t = curTile(); if (!t) { toast('请先选中分区', true); return; }
    const L = curLayout();
    const r = [...t.rect];
    const nt = { id: uid(), cascade: [24, 24], layers: [], rect: [0, 0, 0, 0] };
    if (dir === 'v') {
      t.rect = [r[0], r[1], r[2] / 2, r[3]];
      nt.rect = [r[0] + r[2] / 2, r[1], r[2] / 2, r[3]];
    } else {
      t.rect = [r[0], r[1], r[2], r[3] / 2];
      nt.rect = [r[0], r[1] + r[3] / 2, r[2], r[3] / 2];
    }
    L.tiles.push(nt); selTile = nt.id; selLayer = null;
    renderAll(); scheduleSave(); status('已拆分为 2 个分区');
  }
  function deleteTile() {
    const t = curTile(); if (!t) return;
    const L = curLayout();
    L.tiles = L.tiles.filter((x) => x.id !== t.id);
    selTile = null; selLayer = null;
    renderAll(); scheduleSave();
  }
  function raiseTile() {
    const t = curTile(); if (!t) return;
    const L = curLayout();
    L.tiles = L.tiles.filter((x) => x.id !== t.id).concat([t]);
    renderAll(); scheduleSave();
  }

  /* ---------------- 属性面板 ---------------- */
  function renderPropsValues() {
    const t = curTile(); if (!t) return;
    $('f-x').value = pct(t.rect[0]); $('f-y').value = pct(t.rect[1]);
    $('f-w').value = pct(t.rect[2]); $('f-h').value = pct(t.rect[3]);
    $('f-cx').value = t.cascade[0]; $('f-cy').value = t.cascade[1];
  }
  function renderProps() {
    const t = curTile();
    $('tile-props').classList.toggle('hidden', !!t);
    $('tile-editor').classList.toggle('hidden', !t);
    if (!t) return;
    renderPropsValues();
    const box = $('layer-list');
    box.innerHTML = '';
    if (!t.layers.length) {
      box.innerHTML = '<div class="props-empty" style="padding:6px">暂无应用层</div>';
    }
    t.layers.forEach((ly, i) => {
      const el = document.createElement('div');
      el.className = 'layer-item' + (ly.id === selLayer ? ' active' : '');
      el.innerHTML = `<span class="num">${i + 1}</span><span class="nm">${escapeHtml(ly.name || ly.cmd || '应用')}</span>
        <span class="cmd">${escapeHtml(baseName(ly.cmd))}</span>`;
      el.onclick = () => { selLayer = ly.id; renderCanvas(); renderProps(); };
      el.ondblclick = () => editLayer();
      box.appendChild(el);
    });
  }
  function baseName(p) { return String(p || '').split(/[\\/]/).pop(); }

  ['f-x', 'f-y', 'f-w', 'f-h', 'f-cx', 'f-cy'].forEach((id) => {
    $(id).addEventListener('change', () => {
      const t = curTile(); if (!t) return;
      const v = (k) => parseFloat($(k).value);
      const r = [clamp(v('f-x') / 100, 0, 1), clamp(v('f-y') / 100, 0, 1),
                 clamp(v('f-w') / 100, 0.03, 1), clamp(v('f-h') / 100, 0.03, 1)];
      t.rect = [clamp(r[0], 0, 1 - r[2]), clamp(r[1], 0, 1 - r[3]), r[2], r[3]];
      t.cascade = [Math.round(v('f-cx') || 0), Math.round(v('f-cy') || 0)];
      renderCanvas(); renderPropsValues(); scheduleSave();
    });
  });

  /* ---------------- 应用层编辑 ---------------- */
  function blankLayer() {
    return {
      id: uid(), name: '', cmd: '', args: '', workdir: '',
      process: '', class: '', title: '',
      only_if_missing: S.settings.launch_if_missing !== false,
      fix_style: S.settings.fix_style !== false
    };
  }
  function layerForm(ly) {
    return `
      <div class="form-grid">
        <div class="form-row"><label>显示名称</label><input id="ly-name" value="${escapeAttr(ly.name)}" /></div>
        <div class="form-row"><label>启动命令（exe / lnk / URL / 命令）</label><input id="ly-cmd" value="${escapeAttr(ly.cmd)}" /></div>
        <div class="form-row"><label>启动参数</label><input id="ly-args" value="${escapeAttr(ly.args)}" /></div>
        <div class="form-row"><label>工作目录（可空）</label><input id="ly-workdir" value="${escapeAttr(ly.workdir)}" /></div>
        <div class="form-row"><label>匹配：进程名（如 REDAgent.exe）</label><input id="ly-process" value="${escapeAttr(ly.process)}" /></div>
        <div class="form-row"><label>匹配：窗口类名</label><input id="ly-class" value="${escapeAttr(ly.class)}" /></div>
        <div class="form-row"><label>匹配：标题包含（可空）</label><input id="ly-title" value="${escapeAttr(ly.title)}" /></div>
      </div>
      <label class="chk" style="margin:6px 0"><input type="checkbox" id="ly-missing" ${ly.only_if_missing ? 'checked' : ''}/> 已运行则不重复启动</label>
      <label class="chk"><input type="checkbox" id="ly-fix" ${ly.fix_style ? 'checked' : ''}/> 修复无边框窗口样式（WindowHalf 原有能力）</label>
      <p class="desc" style="margin-top:8px">应用布局时按 进程名 → 类名 → 标题 依次匹配窗口；找不到才启动命令。</p>`;
  }
  function readLayerForm(ly) {
    ly.name = $('ly-name').value.trim();
    ly.cmd = $('ly-cmd').value.trim();
    ly.args = $('ly-args').value.trim();
    ly.workdir = $('ly-workdir').value.trim();
    ly.process = $('ly-process').value.trim();
    ly.class = $('ly-class').value.trim();
    ly.title = $('ly-title').value.trim();
    ly.only_if_missing = $('ly-missing').checked;
    ly.fix_style = $('ly-fix').checked;
    return ly;
  }
  function editLayer() {
    const t = curTile(), ly = curLayer();
    if (!t) { toast('请先选中分区', true); return; }
    const editing = ly ? ly : blankLayer();
    const isNew = !ly;
    openModal(isNew ? '手动添加应用' : '编辑应用层', layerForm(editing), [
      { label: '取消' },
      { label: '保存', primary: true, action: () => {
          readLayerForm(editing);
          if (!editing.cmd && !editing.process) { toast('至少填写启动命令或进程名', true); return false; }
          if (isNew) t.layers.push(editing);
          selLayer = editing.id;
          renderAll(); scheduleSave();
          toast(isNew ? '已添加应用层' : '已更新应用层');
        } }
    ]);
  }
  function pickWindow() {
    const t = curTile();
    if (!t) { toast('请先选中分区', true); return; }
    invoke('list_windows').then((wins) => {
      const rows = wins.map((w) => `
        <label class="win-row">
          <input type="radio" name="pick-win" value="${w.hwnd}" />
          <span class="w-title">${escapeHtml(w.title || '(无标题)')}</span>
          <span class="w-meta">${escapeHtml(w.process)} · ${escapeHtml(w.class)} · ${w.rect[2]}×${w.rect[3]}</span>
        </label>`).join('');
      openModal('从当前窗口添加', `
        <p class="desc">选择一个已打开的窗口：自动写入启动命令（exe 路径）与匹配规则，之后应用布局时可直接启动它。</p>
        <div class="win-list">${rows || '<div class="win-row">没有找到可见窗口</div>'}</div>`,
        [{ label: '取消' }, { label: '添加为应用层', primary: true, action: () => {
            const sel = document.querySelector('input[name="pick-win"]:checked');
            if (!sel) { toast('请先选择一个窗口', true); return false; }
            const w = wins.find((x) => String(x.hwnd) === sel.value);
            if (!w) return false;
            const ly = blankLayer();
            ly.name = (w.title || w.process || '应用').slice(0, 40);
            ly.cmd = w.exe || w.process;
            ly.process = w.process || '';
            ly.class = w.class || '';
            ly.title = '';
            t.layers.push(ly); selLayer = ly.id;
            renderAll(); scheduleSave(); toast('已添加：' + ly.name);
          } }], true);
    }).catch((e) => toast('窗口枚举失败: ' + e, true));
  }
  function layerMove(delta) {
    const t = curTile(), ly = curLayer(); if (!t || !ly) return;
    const i = t.layers.findIndex((x) => x.id === ly.id);
    const j = clamp(i + delta, 0, t.layers.length - 1);
    t.layers.splice(i, 1); t.layers.splice(j, 0, ly);
    renderAll(); scheduleSave();
  }
  function deleteLayer() {
    const t = curTile(), ly = curLayer(); if (!t || !ly) return;
    t.layers = t.layers.filter((x) => x.id !== ly.id);
    selLayer = t.layers.length ? t.layers[t.layers.length - 1].id : null;
    renderAll(); scheduleSave();
  }

  /* ---------------- 保存当前布局 ---------------- */
  function captureDialog() {
    invoke('list_windows').then((wins) => {
      const rows = wins.map((w) => `
        <label class="win-row">
          <input type="checkbox" name="cap-win" value="${w.hwnd}" checked />
          <span class="w-title">${escapeHtml(w.title || '(无标题)')}</span>
          <span class="w-meta">${escapeHtml(w.process)} · ${w.rect[0]},${w.rect[1]} ${w.rect[2]}×${w.rect[3]}</span>
        </label>`).join('');
      const d = new Date();
      const pad = (n) => String(n).padStart(2, '0');
      const defName = `布局 ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
      openModal('★ 保存当前布局', `
        <p class="desc">把桌面上勾选的窗口按当前位置、大小、Z 序保存为一个布局；之后点「应用布局」即可一键打开这些应用并摆回原位。重叠的窗口会自动合并成同一分区的多个层。</p>
        <div class="form-row"><label>布局名称</label><input id="cap-name" value="${escapeAttr(defName)}" /></div>
        <label class="chk" style="margin-bottom:8px"><input type="checkbox" id="cap-group" checked /> 重叠窗口合并为多层布局</label>
        <div class="win-list">${rows || '<div class="win-row">没有找到可见窗口</div>'}</div>`,
        [{ label: '取消' }, { label: '保存布局', primary: true, action: async () => {
            const name = $('cap-name').value.trim();
            if (!name) { toast('请输入布局名称', true); return false; }
            const hwnds = Array.from(document.querySelectorAll('input[name="cap-win"]:checked')).map((n) => Number(n.value));
            if (!hwnds.length) { toast('请至少勾选一个窗口', true); return false; }
            try {
              const st = await invoke('capture_current', { name, hwnds, group_overlap: $('cap-group').checked });
              S = st; curName = name; selTile = null; selLayer = null;
              renderAll(); syncActive(); toast('已保存当前布局：' + name); status('已保存当前布局，共 ' + curLayout().tiles.length + ' 个分区');
            } catch (e) { toast('保存失败: ' + e, true); return false; }
          } }], true);
    }).catch((e) => toast('窗口枚举失败: ' + e, true));
  }

  /* ---------------- 快捷键自定义与占用检测 ---------------- */
  let hkChanged = new Set();
  const hkStatusText = (s) => ({ ok: '可用', occupied: '被占用', empty: '留空', invalid: '格式无效', off: '已关闭' }[s] || s);

  function openHotkeyDialog() {
    const acts = S.hotkey_actions || [];
    hkChanged = new Set();
    acts.forEach((a) => {
      if (Object.prototype.hasOwnProperty.call(S.settings.hotkeys || {}, a.id)) hkChanged.add(a.id);
    });
    const rows = acts.map((a) => {
      const enabled = S.settings.hotkeys_enabled !== false;
      let chip = 'chip off', text = '已关闭', eff = '';
      if (enabled && !a.key) { chip = 'chip empty'; text = '留空'; }
      else if (enabled && a.effective) {
        chip = 'chip ok'; text = '已生效';
        eff = escapeHtml(a.effective);
      } else if (enabled) { chip = 'chip occupied'; text = '未生效'; }
      return `<div class="hk-row">
        <div>${escapeHtml(a.label)}<div class="hk-def">默认 ${escapeHtml(a.default_key)}</div></div>
        <input id="hk-${a.id}" value="${escapeAttr(a.key)}" placeholder="留空 = 禁用" spellcheck="false" />
        <div><span class="${chip}" id="hk-st-${a.id}">${text}</span><span class="hk-eff">${eff}</span></div>
      </div>`;
    }).join('');
    openModal('快捷键设置与占用检测', `
      <p class="desc">格式：<span class="kbd">Win+Alt+K</span>、<span class="kbd">Ctrl+Alt+F12</span>、<span class="kbd">Win+Shift+Left</span>
      （支持 Win/Ctrl/Alt/Shift + 数字、字母、F1~F24、方向键等）。<b>输入框留空即禁用该快捷键</b>；关掉总开关则一个都不注册。
      所有键位（含默认值）都可随意修改，且<b>不会自动回退</b>：被别的程序占用就注册失败，请改键或留空，用「检测占用」查是谁占的。</p>
      <div class="hk-head">
        <label class="chk"><input type="checkbox" id="hk-enabled" ${S.settings.hotkeys_enabled !== false ? 'checked' : ''} /> 启用全部快捷键（总开关）</label>
        <span class="spacer"></span>
        <button id="hk-restore" class="btn tiny">恢复默认</button>
        <button id="hk-clear" class="btn tiny">全部留空</button>
        <button id="hk-detect" class="btn tiny primary">检测占用</button>
      </div>
      <div class="hk-table">${rows}</div>
      <div id="hk-report" class="hk-report"></div>
      <p class="desc" style="margin-top:8px">检测会临时注销本程序的热键再逐个试注册，因此结果不会把「自己」算成占用者。
      Windows 没有公开 API 直接反查 RegisterHotKey 的持有者：工具会给出 WM_SETHOTKEY 的确证结果、
      可疑进程候选，并支持「关闭可疑程序 → 重新检测」的前后对比推断。</p>`,
      [{ label: '关闭' }, { label: '保存设置', primary: true, action: async () => { await saveHotkeys(); return false; } }], true);

    acts.forEach((a) => {
      const el = $('hk-' + a.id);
      if (el) el.addEventListener('input', () => hkChanged.add(a.id));
    });
    $('hk-restore').onclick = () => {
      hkChanged.clear();
      acts.forEach((a) => { const el = $('hk-' + a.id); if (el) el.value = a.default_key; });
      toast('已恢复默认键位（保存后生效）');
    };
    $('hk-clear').onclick = () => {
      acts.forEach((a) => { const el = $('hk-' + a.id); if (el) el.value = ''; hkChanged.add(a.id); });
      toast('已全部留空（保存后禁用所有快捷键）');
    };
    $('hk-detect').onclick = runHotkeyDetect;
  }

  async function runHotkeyDetect() {
    const btn = $('hk-detect');
    if (btn) { btn.disabled = true; btn.textContent = '检测中…'; }
    try {
      const r = await invoke('detect_hotkeys');
      r.items.forEach((it) => {
        const el = $('hk-st-' + it.id);
        if (el) { el.className = 'chip ' + it.status; el.textContent = hkStatusText(it.status); }
      });
      renderHotkeyReport(r);
      status(`快捷键检测：可用 ${r.ok_count} / 被占用 ${r.occupied_count} / 留空 ${r.empty_count}`);
    } catch (e) {
      toast('检测失败：' + e, true);
    } finally {
      if (btn) { btn.disabled = false; btn.textContent = '检测占用'; }
    }
  }

  function renderHotkeyReport(r) {
    const likely = r.likely_owners && r.likely_owners.length ? `
      <div class="likely"><b>推断的占用者</b>：${escapeHtml(r.note)}
        ${r.likely_owners.map((c) => `<div>· ${escapeHtml(c.process)} (PID ${c.pid}) ${escapeHtml(c.exe)}</div>`).join('')}
      </div>` : (r.note ? `<div class="likely">${escapeHtml(r.note)}</div>` : '');
    const confirmed = r.items.filter((i) => i.owners && i.owners.length).map((i) => `
      <div class="cand"><b>${escapeHtml(i.key)}</b> 确证占用者：
        ${i.owners.map((o) => `${escapeHtml(o.process)} (PID ${o.pid}) ${escapeHtml(o.exe)}`).join('；')}
      </div>`).join('');
    const occupied = r.items.filter((i) => i.status === 'occupied').map((i) =>
      `<div class="cand"><span class="chip occupied">被占用</span> ${escapeHtml(i.label)} · ${escapeHtml(i.key)} · ${escapeHtml(i.note)}</div>`).join('');
    const cands = (r.candidates || []).slice(0, 14).map((c) => `
      <div class="cand"><span class="hk-score">${c.score}</span> <b>${escapeHtml(c.process || '(未知)')}</b> PID ${c.pid}
        <span class="hk-meta">${escapeHtml(c.reason)}</span>
        <button class="btn tiny hk-stop" data-pid="${c.pid}" data-force="0" data-proc="${escapeAttr(c.process)}">关闭窗口</button>
        <button class="btn tiny danger-ghost hk-stop" data-pid="${c.pid}" data-force="1" data-proc="${escapeAttr(c.process)}">结束进程</button>
        <div class="hk-path">${escapeHtml(c.exe || c.title)}</div></div>`).join('');
    const box = $('hk-report');
    if (!box) return;
    box.innerHTML = `
      <div class="hk-summary">检测结果：可用 ${r.ok_count} · 被占用 ${r.occupied_count} · 留空 ${r.empty_count} · 格式无效 ${r.invalid_count} · 已关闭 ${r.off_count}
        <button id="hk-redetect" class="btn tiny" style="float:right">重新检测</button></div>
      ${likely}${confirmed}${occupied}
      <div class="hk-sub">可疑进程候选（按可疑度，前 14 个）</div>
      <p class="desc" style="margin:0 0 6px">逐个点「关闭窗口 / 结束进程」（需二次确认），工具会自动重新检测；
      若某个键从被占用变可用，就会把消失的进程标为占用者。系统组件会被拒绝结束。</p>
      ${cands}`;
    $('hk-redetect').onclick = runHotkeyDetect;
    box.querySelectorAll('.hk-stop').forEach((btn) => {
      btn.onclick = async () => {
        const pid = Number(btn.dataset.pid);
        const force = btn.dataset.force === '1';
        const name = btn.dataset.proc || '该进程';
        if (btn.dataset.armed !== '1') {
          box.querySelectorAll('.hk-stop').forEach((b) => {
            b.dataset.armed = '';
            b.textContent = b.dataset.force === '1' ? '结束进程' : '关闭窗口';
          });
          btn.dataset.armed = '1';
          btn.textContent = force ? `确认结束 ${name}？` : `确认关闭 ${name}？`;
          return;
        }
        btn.disabled = true;
        try {
          await invoke('probe_stop_process', { pid, force });
          await new Promise((r) => setTimeout(r, 900));
          await runHotkeyDetect();
        } catch (e) {
          toast('操作失败：' + e, true);
          btn.disabled = false;
          btn.dataset.armed = '';
        }
      };
    });
  }

  async function saveHotkeys() {
    const map = {};
    (S.hotkey_actions || []).forEach((a) => {
      const el = $('hk-' + a.id);
      if (el && hkChanged.has(a.id)) map[a.id] = el.value.trim();
    });
    const settings = Object.assign({}, S.settings, {
      hotkeys_enabled: $('hk-enabled').checked,
      hotkeys: map
    });
    try {
      await invoke('update_settings', { settings });
      S.settings = settings;
      const st = await invoke('get_state');
      S = st;
      toast('快捷键设置已保存并立即生效（点「检测占用」可复查）');
      status('快捷键设置已更新');
    } catch (e) {
      toast('保存失败：' + e, true);
    }
  }

  /* ---------------- 应用 / 设置 ---------------- */
  async function applyLayout() {
    const L = curLayout();
    if (!L) { toast('请先新建一个布局', true); return; }
    if (!L.tiles.length) { toast('布局里没有分区', true); return; }
    await save(false);
    status('正在应用布局：' + L.name + ' …');
    toast('正在应用布局：' + L.name);
    try {
      const report = await invoke('apply_layout', { name: L.name });
      toast('布局已应用 · ' + report.split('\n')[0]);
      status(report.replace(/\n/g, ' | '));
    } catch (e) { toast('应用布局失败: ' + e, true); status('应用失败'); }
  }

  function settingsDialog() {
    const s = S.settings;
    openModal('设置', `
      <div class="form-grid">
        <div class="form-row"><label>贴边吸附阈值（像素）</label><input id="st-snap-px" type="number" value="${s.snap_px ?? 10}" /></div>
        <div class="form-row"><label>守护检查周期（毫秒）</label><input id="st-guard-ms" type="number" value="${s.guard_ms ?? 300}" /></div>
        <div class="form-row"><label>应用启动等待（毫秒）</label><input id="st-wait-ms" type="number" value="${s.wait_ms ?? 9000}" /></div>
      </div>
      <label class="chk"><input type="checkbox" id="st-snap" ${s.snap_enabled !== false ? 'checked' : ''}/> 启用贴边（拖动窗口到屏幕边自动吸附 / Ctrl 拖入分区）</label>
      <label class="chk"><input type="checkbox" id="st-guard" ${s.guard_enabled !== false ? 'checked' : ''}/> 保持状态守护（无边框样式被改回时自动修复；不干预窗口位置/大小/最大化）</label>
      <label class="chk"><input type="checkbox" id="st-fix" ${s.fix_style !== false ? 'checked' : ''}/> 默认修复无边框窗口样式</label>
      <label class="chk"><input type="checkbox" id="st-missing" ${s.launch_if_missing !== false ? 'checked' : ''}/> 已运行的应用不重复启动</label>
      <label class="chk"><input type="checkbox" id="st-autostart" ${S.autostart ? 'checked' : ''}/> 开机自启</label>
      <p class="desc" style="margin-top:10px">
        默认热键可在顶部「快捷键」里逐项自定义、留空禁用、总开关关闭并检测是谁占用了键位。
        关闭窗口会最小化到托盘，托盘菜单可直接应用/保存布局或打开快捷键设置。
      </p>`,
      [{ label: '取消' }, { label: '保存设置', primary: true, action: async () => {
          const ns = {
            snap_px: Number($('st-snap-px').value) || 10,
            guard_ms: Number($('st-guard-ms').value) || 300,
            wait_ms: Number($('st-wait-ms').value) || 9000,
            snap_enabled: $('st-snap').checked,
            guard_enabled: $('st-guard').checked,
            fix_style: $('st-fix').checked,
            launch_if_missing: $('st-missing').checked,
            auto_start: $('st-autostart').checked
          };
          S.settings = ns; S.autostart = ns.auto_start;
          try { await invoke('update_settings', { settings: ns }); toast('设置已保存并生效'); renderAll(); }
          catch (e) { toast('设置保存失败: ' + e, true); return false; }
        } }]);
  }

  /* ---------------- 按钮绑定 ---------------- */
  $('btn-capture').onclick = captureDialog;
  $('btn-apply').onclick = applyLayout;
  $('btn-save').onclick = () => save(true);
  $('btn-settings').onclick = settingsDialog;
  $('btn-hotkeys').onclick = openHotkeyDialog;
  $('btn-new').onclick = () => promptOne('新建布局', '布局名称', '新布局 ' + (S.layouts.length + 1), (name) => {
    if (S.layouts.some((l) => l.name === name)) { toast('同名布局已存在', true); return false; }
    const L = { name, tiles: [
      { id: uid(), rect: [0, 0, 0.5, 1], cascade: [24, 24], layers: [] },
      { id: uid(), rect: [0.5, 0, 0.5, 1], cascade: [24, 24], layers: [] }
    ] };
    S.layouts.push(L); curName = name; selTile = L.tiles[0].id; selLayer = null;
    renderAll(); syncActive(); save(false); status('已新建布局：' + name);
  }, '默认创建左右两个分区，可在画布上拖动调整。');
  $('btn-dup').onclick = () => {
    const L = curLayout(); if (!L) return;
    promptOne('复制布局', '新布局名称', L.name + ' 副本', (name) => {
      if (S.layouts.some((l) => l.name === name)) { toast('同名布局已存在', true); return false; }
      const copy = JSON.parse(JSON.stringify(L));
      copy.name = name;
      copy.tiles.forEach((t) => { t.id = uid(); t.layers.forEach((l) => l.id = uid()); });
      S.layouts.push(copy); curName = name; selTile = null; selLayer = null;
      renderAll(); syncActive(); save(false);
    });
  };
  $('btn-rename').onclick = () => {
    const L = curLayout(); if (!L) return;
    promptOne('重命名布局', '新名称', L.name, (name) => {
      if (name !== L.name && S.layouts.some((l) => l.name === name)) { toast('同名布局已存在', true); return false; }
      L.name = name; curName = name; renderAll(); syncActive(); save(false);
    });
  };
  $('btn-delete').onclick = () => {
    const L = curLayout(); if (!L) return;
    confirmBox('删除布局', `确定删除「${escapeHtml(L.name)}」？此操作不可撤销。`, () => {
      S.layouts = S.layouts.filter((x) => x.name !== L.name);
      curName = S.layouts.length ? S.layouts[0].name : null;
      selTile = null; selLayer = null; renderAll(); syncActive(); save(false);
    });
  };
  $('btn-split-v').onclick = () => splitTile('v');
  $('btn-split-h').onclick = () => splitTile('h');
  $('btn-add-tile').onclick = () => addTile([0.2, 0.2, 0.4, 0.5]);
  $('chk-snap').onchange = () => {
    S.settings.snap_enabled = $('chk-snap').checked;
    invoke('update_settings', { settings: S.settings }).catch(() => {});
  };
  $('btn-add-window').onclick = pickWindow;
  $('btn-add-manual').onclick = editLayer;
  $('btn-layer-up').onclick = () => layerMove(1);
  $('btn-layer-down').onclick = () => layerMove(-1);
  $('btn-layer-top').onclick = () => {
    const t = curTile(), ly = curLayer(); if (!t || !ly) return;
    t.layers = t.layers.filter((x) => x.id !== ly.id).concat([ly]);
    renderAll(); scheduleSave();
  };
  $('btn-layer-edit').onclick = editLayer;
  $('btn-layer-del').onclick = deleteLayer;

  // 后端（托盘/热键/守护）改动布局时刷新
  if (T && T.event && T.event.listen) {
    T.event.listen('layouts-changed', () => load(true)).catch(() => {});
    T.event.listen('winlayout-log', (e) => { try { status(String(e.payload)); } catch (_) {} }).catch(() => {});
    T.event.listen('open-hotkeys', () => openHotkeyDialog()).catch(() => {});
  }
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') { closeModal(); hideContextMenu(); }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's') { e.preventDefault(); save(true); }
  });

  load(false);
})();
