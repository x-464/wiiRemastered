import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { readTextFile } from '@tauri-apps/plugin-fs';
import { appDataDir, join } from "@tauri-apps/api/path";
import { convertFileSrc } from "@tauri-apps/api/core";
import { LazyStore } from "@tauri-apps/plugin-store";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

let games = [];
let curPage = 1;
let dolphinPath = null;
let gamesPath = null;

const store = new LazyStore("settings.json");

const fileDir = document.querySelector(".settingsBtnSVGLeft");
const dolphinDir = document.querySelector(".settingsBtnSVGRight");
const scrollTrack = document.querySelector(".scrollTrack");
const arrowLeft = document.querySelector(".arrowWrapL");
const arrowRight = document.querySelector(".arrowWrapR");

const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");


async function getGameInfo() {
    games = [];

    const selected = await open({ directory: true, multiple: false });
    if (!selected || Array.isArray(selected)) { return; }
    gamesPath = await readDir(selected);

    const isoPaths = gamesPath
        .filter(entry => entry.isFile && entry.name?.toLowerCase().endsWith('.iso'))
        .map(entry => `${selected}${selected.endsWith('\\') || selected.endsWith('/') ? '' : '/'}${entry.name}`);

    for (const isoPath of isoPaths) {
        const id = await invoke('get_id', { path: isoPath });
        const title = await getTitle(id);
        const folderKey = safeFolderKey(title, id);

        const bnrPath = await invoke("unwrap_iso", { isoPath });
        const binPaths = await invoke("unwrap_bnr", { bnrPath });

        let pngPaths = [];
        let pngPath = null;
        let jsonPath = null;
        let animPath = null;

        for (const binPath of binPaths) {
            if (binPath.bin_path.toLowerCase().endsWith("banner.bin") || binPath.bin_path.toLowerCase().endsWith("icon.bin")) {
                const imgInfoPaths = await invoke("unwrap_bin", { binPath: binPath.bin_path });
                for (const imgInfoPath of imgInfoPaths) {
                    if (imgInfoPath.source_path.toLowerCase().endsWith(".tpl") || imgInfoPath.source_path.toLowerCase().endsWith(".tex0")) {
                        pngPath = await invoke("tpl_to_png", { tplPath: imgInfoPath.source_path, title: folderKey });
                    }
                    if (imgInfoPath.source_path.toLowerCase().endsWith("icon.brlyt")) {
                        jsonPath = await invoke("convert_brlyt", { brlytPath: imgInfoPath.source_path, title: folderKey });
                    }
                    if (imgInfoPath.source_path.toLowerCase().endsWith("icon.brlan")) {
                        animPath = await invoke("convert_brlan", { brlanPath: imgInfoPath.source_path, title: folderKey });
                    }
                    pngPaths.push(pngPath);
                }
            }
        }

        const game = { id, title, folderKey, isoPath, bnrPath, binPaths, pngPaths, jsonPath, animPath };
        games.push(game);
    }
}

async function getDolphinPath() {
    const isMac = navigator.platform?.startsWith("Mac")
        || navigator.userAgent.includes("Macintosh");

    const selected = await open({
        multiple: false,
        filters: [
            {
                name: "Dolphin",
                // macOS Dolphin is a .app bundle; the Rust side resolves it
                // to the real binary inside Contents/MacOS
                extensions: isMac ? ["app"] : ["exe"]
            }
        ]
    });

    if (!selected || Array.isArray(selected)) return;

    dolphinPath = selected;
}

// ---- per-vertex color (GX Gouraud) support ----
// GX bilinearly interpolates the 4 corner colors across the quad and multiplies
// them with the texture. All banner icons observed only vary corner ALPHA (RGB
// stays white), which maps exactly onto an SVG luminance mask built from a
// small bilinearly-filled bitmap.

const vtxMaskCache = new Map();
let warnedVtxRGB = false;

function vertexAlphaMaskURI(alphas) {
    const key = alphas.join("|");
    const cached = vtxMaskCache.get(key);
    if (cached) return cached;

    const N = 64;
    const [tlA, trA, blA, brA] = alphas;
    const canvas = document.createElement("canvas");
    canvas.width = N;
    canvas.height = N;
    const ctx = canvas.getContext("2d");
    const pixels = ctx.createImageData(N, N);

    for (let y = 0; y < N; y++) {
        const v = y / (N - 1);
        for (let x = 0; x < N; x++) {
            const u = x / (N - 1);
            const a = (tlA * (1 - u) + trA * u) * (1 - v)
                    + (blA * (1 - u) + brA * u) * v;
            const o = (y * N + x) * 4;
            pixels.data[o] = 255;
            pixels.data[o + 1] = 255;
            pixels.data[o + 2] = 255;
            pixels.data[o + 3] = Math.round(a);
        }
    }

    ctx.putImageData(pixels, 0, 0);
    const uri = canvas.toDataURL("image/png");
    vtxMaskCache.set(key, uri);
    return uri;
}

function vertexCornerInfo(pane) {
    const tl = pane.top_left_color;
    const tr = pane.top_right_color;
    const bl = pane.bottom_left_color;
    const br = pane.bottom_right_color;
    if (!tl || !tr || !bl || !br) return { mul: 1, maskURI: null };

    if (!warnedVtxRGB && [tl, tr, bl, br].some(c => c[0] !== 255 || c[1] !== 255 || c[2] !== 255)) {
        warnedVtxRGB = true;
        console.warn("vertex colors with non-white RGB found; only their alpha is rendered");
    }

    const alphas = [tl[3], tr[3], bl[3], br[3]];
    if (alphas.every(a => a === alphas[0])) {
        return { mul: alphas[0] / 255, maskURI: null };
    }
    return { mul: 1, maskURI: vertexAlphaMaskURI(alphas) };
}

async function makePane(pane, parentEl, gameNum, reg) {
    const SVG_NS = "http://www.w3.org/2000/svg";

    const vtx = vertexCornerInfo(pane);

    const group = document.createElementNS(SVG_NS, "g");
    group.setAttribute("transform", `translate(${pane.x}, ${pane.y}) scale(${pane.scale_x}, ${pane.scale_y})`);
    group.setAttribute("opacity", `${(pane.alpha / 255) * vtx.mul}`);
    // hidden panes still get built so RLVI (visibility) animation can reveal them
    if (!pane.visible) group.style.display = "none";

    parentEl.appendChild(group);

    let img = null;
    if (pane.type === "pic1" && pane.png_candidate){
        img = document.createElementNS(SVG_NS, "image");

        const appDataPath = await appDataDir();
        const fullPngPath = await join(appDataPath, "wiiMainMenu", "cached_pngs", `${games[gameNum].folderKey}/`, pane.png_candidate);
        const assetUrl = convertFileSrc(fullPngPath);
        img.setAttribute("href", assetUrl);
        img.setAttribute("x", `${-pane.width / 2}`);
        img.setAttribute("y", `${-pane.height / 2}`);
        img.setAttribute("width", `${pane.width}`);
        img.setAttribute("height", `${pane.height}`);
        img.setAttribute("preserveAspectRatio", "none");
        img.setAttribute("transform", "scale(1,-1)");

        if (vtx.maskURI && reg?.defs) {
            const maskId = `vtx_${reg.uid}_${pane.name.replace(/[^\w-]/g, "_")}`;
            const mask = document.createElementNS(SVG_NS, "mask");
            mask.setAttribute("id", maskId);
            mask.setAttribute("maskUnits", "userSpaceOnUse");
            mask.setAttribute("x", `${-pane.width / 2}`);
            mask.setAttribute("y", `${-pane.height / 2}`);
            mask.setAttribute("width", `${pane.width}`);
            mask.setAttribute("height", `${pane.height}`);

            const maskImg = document.createElementNS(SVG_NS, "image");
            maskImg.setAttribute("href", vtx.maskURI);
            maskImg.setAttribute("x", `${-pane.width / 2}`);
            maskImg.setAttribute("y", `${-pane.height / 2}`);
            maskImg.setAttribute("width", `${pane.width}`);
            maskImg.setAttribute("height", `${pane.height}`);
            maskImg.setAttribute("preserveAspectRatio", "none");

            mask.appendChild(maskImg);
            reg.defs.appendChild(mask);
            img.setAttribute("mask", `url(#${maskId})`);
        }

        group.appendChild(img);
    }

    if (reg) {
        const rec = { el: group, img, pane, vtxMul: vtx.mul };
        if (pane.name && !reg.panes.has(pane.name)) reg.panes.set(pane.name, rec);
        if (pane.material_name) {
            if (!reg.mats.has(pane.material_name)) reg.mats.set(pane.material_name, []);
            reg.mats.get(pane.material_name).push(rec);
        }
    }

    for (const child of pane.children || []){
        await makePane(child, group, gameNum, reg);
    }
}

// ---- TV static for empty channels ----
// Pre-renders a few frames of speckle noise once at startup; the render loop
// cycles the shared frames across all null cards and scrolls their scanline
// pattern. Frames are white-with-alpha, so plain compositing over the stripes
// gives exactly the screen blend the old feTurbulence filter produced.

const SCAN_PERIOD_MS = 1500;  // one 5-unit scanline period per 1.5s (as before)
const NOISE_INTERVAL_MS = 60; // ~30 flickers/sec, the old filter's effective rate

function makeNoiseFrames(frameCount = 8) {
    const W = 200, H = 113;
    const NW = 300, NH = 163;

    const small = document.createElement("canvas");
    small.width = NW;
    small.height = NH;
    const sctx = small.getContext("2d");

    const big = document.createElement("canvas");
    big.width = W;
    big.height = H;
    const bctx = big.getContext("2d");
    bctx.imageSmoothingEnabled = true;

    const frames = [];
    for (let f = 0; f < frameCount; f++) {
        // triangular-distributed noise through a soft contrast curve; keeps
        // speckles sparse and faint like the old feTurbulence, not white blobs
        const px = sctx.createImageData(NW, NH);
        for (let i = 0; i < NW * NH; i++) {
            const n = (Math.random() + Math.random()) / 2;
            // the trailing multiplier caps how bright a speckle can get over
            // the scanline base — raise/lower it to make the static louder/softer
            const a = Math.min(1, Math.max(0, n * 3 - 1.1)) * 120;
            const o = i * 4;
            px.data[o] = 255;
            px.data[o + 1] = 255;
            px.data[o + 2] = 255;
            px.data[o + 3] = a;
        }
        sctx.putImageData(px, 0, 0);

        bctx.clearRect(0, 0, W, H);
        bctx.drawImage(small, 0, 0, W, H);
        frames.push(big.toDataURL("image/png"));
    }
    return frames;
}

let noiseFrames = null;
let noiseImgs = [];
let scanRects = [];
let noiseIdx = 0;
let lastNoiseSwap = 0;

// hovered card's pulse <g>, sampled every frame by the render loop so the
// unhover handler knows the exact mid-pulse scale to hand to the transition
// (reading it inside mouseleave is too late: :hover has already unmatched)
let hoveredPulse = null;
let hoveredPulseScale = "none";

// ---- wiimote-driven hover ----
// CSS :hover only follows the real mouse, so the cursor loop hit-tests the
// wiimote position and mirrors hover as a .wiiHover class (styling) plus
// real mouseenter/mouseleave events on game cards (tooltip + pulse JS).

const WII_HOVERABLE = ".game.filled, .arrowWrap, .settingsBtnSVG";
let wiiHovered = new Set();

function updateWiiHover(px, py, visible) {
    const next = new Set();

    if (visible) {
        // collect the whole hoverable ancestor chain, like real hover does
        let el = document.elementFromPoint(px, py);
        while (el) {
            el = el.closest(WII_HOVERABLE);
            if (!el) break;
            next.add(el);
            el = el.parentElement;
        }
    }

    for (const el of wiiHovered) {
        if (!next.has(el)) {
            el.classList.remove("wiiHover");
            if (el.matches(".game.filled")) el.dispatchEvent(new MouseEvent("mouseleave"));
        }
    }
    for (const el of next) {
        if (!wiiHovered.has(el)) {
            el.classList.add("wiiHover");
            if (el.matches(".game.filled")) el.dispatchEvent(new MouseEvent("mouseenter"));
        }
    }

    wiiHovered = next;
}

function collectNoiseImgs() {
    noiseImgs = [...document.querySelectorAll(".gameGrid .game .nullNoise")];
    scanRects = [...document.querySelectorAll(".gameGrid .game .nullScan")];
    if (noiseFrames) {
        for (const el of noiseImgs) el.setAttribute("href", noiseFrames[0]);
    }
}

// ---- banner animation (BRLAN) runtime ----

const activeAnimators = [];
let animEpoch = performance.now();

function evalTrack(track, frame) {
    const kfs = track.keyframes;

    if (track.data_type === 1) { // step
        let v = kfs[0].value;
        for (const k of kfs) {
            if (k.frame <= frame) v = k.value;
            else break;
        }
        return v;
    }

    // hermite
    if (kfs.length === 1 || frame <= kfs[0].frame) return kfs[0].value;
    const last = kfs[kfs.length - 1];
    if (frame >= last.frame) return last.value;

    let i = 0;
    while (kfs[i + 1].frame <= frame) i++;
    const k0 = kfs[i];
    const k1 = kfs[i + 1];
    const d = k1.frame - k0.frame;
    if (d <= 0) return k1.value;

    const t = (frame - k0.frame) / d;
    const t2 = t * t;
    const t3 = t2 * t;
    return (2 * t3 - 3 * t2 + 1) * k0.value
         + (t3 - 2 * t2 + t) * d * k0.slope
         + (-2 * t3 + 3 * t2) * k1.value
         + (t3 - t2) * d * k1.slope;
}

function applyAnimItem(item, frame, texHrefs) {
    const { el, img, pane } = item.rec;
    const p = item.paneProps;
    const get = (name, fallback) => (p && p[name]) ? evalTrack(p[name], frame) : fallback;

    const x = get("trans_x", pane.x);
    const y = get("trans_y", pane.y);
    const rz = get("rot_z", pane.rot_z || 0);
    const sx = get("scale_x", pane.scale_x);
    const sy = get("scale_y", pane.scale_y);

    const transform = `translate(${x}, ${y}) rotate(${rz}) scale(${sx}, ${sy})`;
    if (transform !== item.last.transform) {
        el.setAttribute("transform", transform);
        item.last.transform = transform;
    }

    // pane alpha track replaces the base alpha; vertex/material alphas multiply on top
    let alpha = (get("alpha", pane.alpha) / 255) * (item.rec.vtxMul ?? 1);
    if (p) {
        const corners = ["vtx_lt_a", "vtx_rt_a", "vtx_lb_a", "vtx_rb_a"].filter(k => p[k]);
        if (corners.length) {
            let sum = 0;
            for (const k of corners) sum += evalTrack(p[k], frame);
            alpha *= (sum / corners.length) / 255;
        }
    }
    for (const m of item.matProps) {
        if (m.mat_a) alpha *= evalTrack(m.mat_a, frame) / 255;
    }
    const alphaStr = Math.max(0, Math.min(1, alpha)).toFixed(3);
    if (alphaStr !== item.last.alpha) {
        el.setAttribute("opacity", alphaStr);
        item.last.alpha = alphaStr;
    }

    if (p && p.visible) {
        const vis = evalTrack(p.visible, frame) >= 0.5;
        if (vis !== item.last.visible) {
            el.style.display = vis ? "" : "none";
            item.last.visible = vis;
        }
    }

    if (img && p && (p.size_w || p.size_h)) {
        const w = get("size_w", pane.width);
        const h = get("size_h", pane.height);
        const size = `${w}x${h}`;
        if (size !== item.last.size) {
            img.setAttribute("x", `${-w / 2}`);
            img.setAttribute("y", `${-h / 2}`);
            img.setAttribute("width", `${w}`);
            img.setAttribute("height", `${h}`);
            item.last.size = size;
        }
    }

    if (img && p && p.tex_pattern && texHrefs) {
        const ti = Math.round(evalTrack(p.tex_pattern, frame));
        if (ti !== item.last.tex && texHrefs[ti]) {
            img.setAttribute("href", texHrefs[ti]);
            item.last.tex = ti;
        }
    }
}

async function registerBannerAnimation(anim, reg, gameNum) {
    const items = new Map(); // element -> item, so pane + material entries merge

    for (const entry of anim.entries || []) {
        const props = {};
        for (const tag of entry.tags || []) {
            for (const target of tag.targets || []) {
                if (target.keyframes?.length) props[target.property] = target;
            }
        }
        if (!Object.keys(props).length) continue;

        const recs = entry.is_material
            ? (reg.mats.get(entry.name) || [])
            : (reg.panes.has(entry.name) ? [reg.panes.get(entry.name)] : []);

        for (const rec of recs) {
            let item = items.get(rec.el);
            if (!item) {
                item = { rec, paneProps: null, matProps: [], last: {} };
                items.set(rec.el, item);
            }
            if (entry.is_material) item.matProps.push(props);
            else item.paneProps = props;
        }
    }

    if (!items.size) return;

    // pre-resolve texture-pattern (RLTP) swap targets to asset urls
    let texHrefs = null;
    if (anim.textures?.length) {
        const appDataPath = await appDataDir();
        texHrefs = [];
        for (const name of anim.textures) {
            const stem = name.replace(/\.(tpl|tex0)$/i, "");
            const full = await join(appDataPath, "wiiMainMenu", "cached_pngs", `${games[gameNum].folderKey}/`, `${stem}.png`);
            texHrefs.push(convertFileSrc(full));
        }
    }

    activeAnimators.push({
        frameCount: Math.max(1, anim.frame_count || 1),
        loops: anim.loop !== false,
        items: [...items.values()],
        texHrefs,
        page: Math.floor(gameNum / 12),
    });
}

function startBannerAnimationLoop() {
    function tick(now) {
        const page = curPage - 1; // only the visible page pays rendering cost

        if (hoveredPulse) {
            hoveredPulseScale = getComputedStyle(hoveredPulse).scale;
        }

        const elapsed = (now - animEpoch) / 1000 * 60; // banners run at 60 anim-frames/sec
        for (const a of activeAnimators) {
            if (a.page !== page) continue;
            const frame = a.loops ? elapsed % a.frameCount : Math.min(elapsed, a.frameCount - 1);
            for (const item of a.items) applyAnimItem(item, frame, a.texHrefs);
        }

        // static runs on every page (cheap: shared pre-decoded frames, and
        // neighbouring pages peek in at grid edges). Random frame order so
        // the small pool never reads as a loop.
        if (noiseFrames) {
            if (now - lastNoiseSwap >= NOISE_INTERVAL_MS) {
                lastNoiseSwap = now;
                noiseIdx = (noiseIdx + 1 + Math.floor(Math.random() * (noiseFrames.length - 1))) % noiseFrames.length;
                const href = noiseFrames[noiseIdx];
                for (const el of noiseImgs) {
                    el.setAttribute("href", href);
                }
            }
            // scanlines drift up one 5-unit period per SCAN_PERIOD_MS,
            // updated every frame so the motion stays smooth
            const scanY = -((now % SCAN_PERIOD_MS) / SCAN_PERIOD_MS) * 5;
            const scanTr = `translate(0 ${scanY.toFixed(3)})`;
            for (const el of scanRects) {
                el.setAttribute("transform", scanTr);
            }
        }

        requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
}

async function insertNullSVG(gameNum) {
    const nullSVG = await fetch("/assets/svgs/nullGame.svg");
    const nullText = await nullSVG.text();

    const wrapper = document.createElement("div");
    wrapper.setAttribute("class", `game ${gameNum}`);
    wrapper.dataset.page = String(Math.floor(gameNum / 12));
    wrapper.innerHTML = nullText;

    return wrapper;
}

function findBackgroundPic(panes) {
    for (const p of panes) {
        if (p.type === "pic1" && p.png_candidate) return p;
        const found = findBackgroundPic(p.children || []);
        if (found) return found;
    }
    return null;
}

// rounded TV-frame outline shared by the border stroke and the texture clip
const CHANNEL_BORDER_D = "M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z";

async function insertGameSVG(gameNum) {
    const res = await readTextFile(games[gameNum].jsonPath);
    const channelJson = JSON.parse(res);
    const SVG_NS = "http://www.w3.org/2000/svg";

    const wrapper = document.createElement("div");
    wrapper.setAttribute("class", `game filled ${gameNum}`);
    wrapper.dataset.page = String(Math.floor(gameNum / 12));

    const gameSVG = document.createElementNS(SVG_NS, "svg");
    gameSVG.setAttribute("viewBox", "0 0 200 113");
    gameSVG.setAttribute("class", "gamePath");

    // nested SVG for the channel layout, inset to sit inside the rounded frame
    const bg = findBackgroundPic(channelJson.root);
    const vw = bg ? bg.width * (bg.scale_x || 1) : (channelJson.width || 200);
    const vh = bg ? bg.height * (bg.scale_y || 1) : (channelJson.height || 113);

    // viewport matches the border path bounds (190 x ~103); "slice" scales the
    // content up uniformly until it covers the frame (no stretching), and the
    // rounded clip + viewport crop the slight vertical overflow
    const layoutSvg = document.createElementNS(SVG_NS, "svg");
    layoutSvg.setAttribute("x", "5");
    layoutSvg.setAttribute("y", "6");
    layoutSvg.setAttribute("width", "190");
    layoutSvg.setAttribute("height", "103.2");
    layoutSvg.setAttribute("viewBox", `${-vw / 2} ${-vh / 2} ${vw} ${vh}`);
    layoutSvg.setAttribute("preserveAspectRatio", "xMidYMid slice");

    const preciseGroup = document.createElementNS(SVG_NS, "g");
    preciseGroup.setAttribute("transform", "scale(1,-1)"); // BRLYT is Y-up
    layoutSvg.appendChild(preciseGroup);

    // Clip the channel content to the rounded TV frame; the grey stroke is
    // drawn on top afterwards so the edge stays crisp.
    // IMPORTANT: the clip-path must sit on a <g> WRAPPING the nested <svg>.
    // Putting it directly on the nested <svg> makes WebView2 leak a raster
    // surface every animation frame (~400 MB/s -> renderer OOM crash).
    const defs = document.createElementNS(SVG_NS, "defs");
    const clip = document.createElementNS(SVG_NS, "clipPath");
    clip.setAttribute("id", `chanClip_g${gameNum}`);
    const clipShape = document.createElementNS(SVG_NS, "path");
    clipShape.setAttribute("d", CHANNEL_BORDER_D);
    clipShape.setAttribute("transform", "translate(5 6)");
    clip.appendChild(clipShape);
    defs.appendChild(clip);
    gameSVG.appendChild(defs);

    const clipGroup = document.createElementNS(SVG_NS, "g");
    clipGroup.setAttribute("clip-path", `url(#chanClip_g${gameNum})`);
    clipGroup.appendChild(layoutSvg);
    gameSVG.appendChild(clipGroup);

    const reg = { panes: new Map(), mats: new Map(), defs, uid: `g${gameNum}` };

    for (const pane of channelJson.root) {
        await makePane(pane, preciseGroup, gameNum, reg);
    }

    // animPath may be missing on games scanned before animation support existed;
    // the anim json always sits next to the layout json, so fall back to that
    const animPath = games[gameNum].animPath
        ?? games[gameNum].jsonPath?.replace(/\.json$/, "_anim.json");
    if (animPath) {
        try {
            const animText = await readTextFile(animPath);
            await registerBannerAnimation(JSON.parse(animText), reg, gameNum);
        } catch (err) {
            console.warn("banner animation failed for", games[gameNum].title, err);
        }
    }

    wrapper.appendChild(gameSVG);

    const border = document.createElementNS(SVG_NS, "path");
    border.setAttribute("transform", "translate(5 6)");
    border.setAttribute("d", CHANNEL_BORDER_D);
    border.setAttribute("fill", "none");
    border.setAttribute("stroke", "#bbbbbb");
    border.setAttribute("stroke-width", "2");
    gameSVG.appendChild(border);

    // hover glow: same frame shape drawn above the grey border. The wrapper
    // <g> carries the looping pulse animation while the path itself carries
    // the enter/exit transition, so the two never fight over one property.
    const hoverPulse = document.createElementNS(SVG_NS, "g");
    hoverPulse.setAttribute("class", "hoverPulse");
    const hoverStroke = document.createElementNS(SVG_NS, "path");
    hoverStroke.setAttribute("class", "hoverStroke");
    hoverStroke.setAttribute("transform", "translate(5 6)");
    hoverStroke.setAttribute("d", CHANNEL_BORDER_D);
    hoverStroke.setAttribute("fill", "none");
    hoverPulse.appendChild(hoverStroke);
    gameSVG.appendChild(hoverPulse);

    return wrapper;
}

async function insertChannels(){
    let remainingGames = games.length;
    const gameGrids = [gameGridP1, gameGridP2, gameGridP3];

    const page1Games = Math.min(remainingGames, 12);
    remainingGames -= page1Games;
    const page2Games = Math.min(remainingGames, 12);
    remainingGames -= page2Games;
    const page3Games = Math.min(remainingGames, 12);
    remainingGames -= page3Games;

    const spacesTaken = [page1Games, page2Games, page3Games];

    activeAnimators.length = 0; // old SVG nodes are about to be wiped
    animEpoch = performance.now();

    gameGridP1.innerHTML = "";
    gameGridP2.innerHTML = "";
    gameGridP3.innerHTML = "";

    for (const grid of gameGrids){
        grid.style.visibility = "hidden";
    }

    for (let i = 0; i < 3; i++) {
        for (let j = 0; j < 12; j++) {
            const gameIndex = i * 12 + j;

            if (j < spacesTaken[i]) {
                // a game with missing/stale cache files must degrade to an
                // empty slot, never break the whole menu
                try {
                    gameGrids[i].appendChild(await insertGameSVG(gameIndex));
                } catch (err) {
                    console.warn("game card failed, showing empty slot:", games[gameIndex]?.title, err);
                    gameGrids[i].appendChild(await insertNullSVG(gameIndex));
                }
            }
            else {
                gameGrids[i].appendChild(await insertNullSVG(gameIndex));
            }
        }
    }

    for (const grid of gameGrids) {
        grid.style.visibility = "visible";
    }

    collectNoiseImgs();
}

async function getTitle(id) {
    let title = "";

    let db = await fetch("/assets/db/wiitdb.xml");
    let dbText = await db.text();
    const parser = new DOMParser();
    const xmlParsed = parser.parseFromString(dbText, "application/xml");
    const games = xmlParsed.getElementsByTagName("game");

    for (const game of games) {
        const currentID = game.getElementsByTagName("id")[0]?.textContent.trim();

        if (currentID === id) {
            title = game.getElementsByTagName("title")[0]?.textContent.trim();
            break;
        }
    }

    return title;
}

async function attachListenersToGames() {
    const gameCards = document.querySelectorAll(".game");
    const topWrapper = document.querySelector(".topWrapper");
    const gameTitleWrapper = document.querySelector(".gameTitleWrapper");
    const gameTitle = document.querySelector(".gameTitle");

    // one shared tooltip -> one shared pending-show timer, so a stale timer
    // can never re-show the tooltip after mouseleave hid it
    let titleShowTimer = null;

    for (let i = 0; i < games.length; i++) {
        gameCards[i].addEventListener("mouseenter", () => {
            const gameCard = gameCards[i];
            const currentGrid = gameCard.closest(".gameGrid");

            hoveredPulse = gameCard.querySelector(".hoverPulse");
            hoveredPulseScale = "none";

            const gameRect = gameCard.getBoundingClientRect();
            const wrapperRect = topWrapper.getBoundingClientRect();
            const gridRect = currentGrid.getBoundingClientRect();

            const x = gameRect.left - wrapperRect.left + gameRect.width / 2;
            const y = gameRect.top - wrapperRect.top + gameRect.height - 7;

            clearTimeout(titleShowTimer);
            titleShowTimer = setTimeout(() => {
                gameTitle.textContent = games[i].title;
                gameTitleWrapper.style.display = "flex";

                // Position via left/top ONLY — transform and scale belong to
                // the CSS animations. offsetWidth is the un-scaled layout
                // width, so a mid-flight scale/transform animation can't
                // corrupt the centering or edge-correction math.
                const w = gameTitleWrapper.offsetWidth;
                const gutter = 8;
                const gridLeft = gridRect.left - wrapperRect.left;
                const gridRight = gridRect.right - wrapperRect.left;

                let leftX = x - w / 2;
                if (leftX < gridLeft + gutter) leftX = gridLeft + gutter;
                if (leftX + w > gridRight - gutter) leftX = gridRight - gutter - w;

                gameTitleWrapper.style.left = `${leftX}px`;
                gameTitleWrapper.style.top = `${y}px`;
            }, 250);
        });

        gameCards[i].addEventListener("mouseleave", () => {
            clearTimeout(titleShowTimer);
            gameTitleWrapper.style.display = "none";

            // hand the pulse's live scale to the CSS transition: pin it for
            // one frame (the animation is already gone), then release so it
            // eases back to neutral instead of snapping
            if (hoveredPulse && hoveredPulseScale !== "none") {
                const pulse = hoveredPulse;
                pulse.style.scale = hoveredPulseScale;
                requestAnimationFrame(() => { pulse.style.scale = ""; });
            }
            hoveredPulse = null;
            hoveredPulseScale = "none";
        });

        gameCards[i].addEventListener("click", async () => {
            console.log("launching with dolphin:", dolphinPath, "iso:", games[i].isoPath);
            await invoke("open_game", { gamePath: games[i].isoPath, dolphinPath: String(dolphinPath) });
            stopWiimote();
        });
    }
}

function scrollToPage(container, page) {
    const target =
        page.offsetLeft -
        (container.clientWidth / 2) +
        (page.clientWidth / 2);

    container.scrollTo({
        left: target,
        behavior: "smooth"
    });
}

function arrowShow() {
    if (curPage == 1) {
        arrowLeft.style.visibility = "hidden";
    } else {
        arrowLeft.style.visibility = "visible";
    }
    if (curPage == 3) {
        arrowRight.style.visibility = "hidden";
    } else {
        arrowRight.style.visibility = "visible";
    }
}

function clickAnimation(){
    const arrowMinus = document.querySelector(".arrowMinus");
    const arrowPlus = document.querySelector(".arrowPlus");

    arrowLeft.addEventListener("click", () => {
        arrowMinus.classList.add("arrowBtnClick");
        setTimeout(() => {
            arrowMinus.classList.remove("arrowBtnClick");
        }, 200)
    })

    arrowRight.addEventListener("click", () => {
        arrowPlus.classList.add("arrowBtnClick");
        setTimeout(() => {
            arrowPlus.classList.remove("arrowBtnClick");
        }, 200)
    })
}

function safeFolderKey(title, id) {
    const cleaned = (title || "")
        .replace(/[<>:"/\\|?*\x00-\x1F]/g, "_")
        .replace(/[. ]+$/g, "")
        .trim();
    return cleaned || id;
}

async function startWiimote() {
    try { await invoke("start_wiimote"); }
    catch (err) { console.error("start_wiimote failed:", err); }
}

async function initWiimote() {
    const wii = document.querySelector(".wiiGrid");
    const cursorP1 = document.querySelector(".cursorP1");

    let latest = null;

    function onAPressed(nx, ny) {
        const wii = document.querySelector(".wiiGrid").getBoundingClientRect();

        const wiiX = wii.left + nx * wii.width;
        const wiiY = wii.top + ny * wii.height;

        const target = document.elementFromPoint(wiiX, wiiY);

        console.log(wiiX, wiiY);
        console.log(target);
        target.click();
    }

    await listen("wiimote-update", (e) => { latest = e.payload; });

    await listen("wiimote-button", (e) => {
        const { button, pressed, x, y, visible } = e.payload;
        if (button === "a" && pressed) onAPressed(x, y);
    });

    await listen("wiimote-error", (e) => console.error("Rust wiimote error:", e.payload));
    await listen("wiimote-connected", (e) => console.log(`Player ${e.payload} connected`));
    await listen("wiimote-disconnected", (e) => {
        console.log(`Player ${e.payload} disconnected`);
        cursorP1.style.opacity = "0";
        latest = null;
        updateWiiHover(0, 0, false);
    });

    await listen("game-error", (e) => {
        console.error("open_game failed:", e.payload);
        startWiimote();
    });

    await listen("game-closed", () => { startWiimote(); });

    function renderCursor() {
        if (latest) {
            const rect = wii.getBoundingClientRect();
            const { x, y, visible, rotation } = latest;
            if (visible) {
                cursorP1.style.opacity = "1";
                cursorP1.style.transform =
                    `translate(${x * rect.width}px, ${y * rect.height}px) translate(-50%, -50%) rotate(${rotation}deg)`;
                updateWiiHover(rect.left + x * rect.width, rect.top + y * rect.height, true);
            } else {
                cursorP1.style.opacity = "0";
                updateWiiHover(0, 0, false);
            }
        }
        requestAnimationFrame(renderCursor);
    }
    requestAnimationFrame(renderCursor);
}

async function stopWiimote(){
    updateWiiHover(0, 0, false); // release any wiimote-held hover states
    await invoke("stop_wiimote");
}

async function onStart() {
    dolphinPath = await store.get("dolphinPath") ?? null;
    gamesPath = await store.get("gamesPath") ?? null;
    games = await store.get("games") ?? null;
    if (!Array.isArray(games)) games = [];

    await insertChannels();
    await attachListenersToGames();
    await initWiimote();
    await startWiimote();
}

async function onClose() {
    const appWindow = getCurrentWindow();

    await appWindow.onCloseRequested(async (event) => {
        event.preventDefault();
        try {
            stopWiimote();
            await store.set("dolphinPath", dolphinPath ?? null);
            await store.set("gamesPath", gamesPath ?? null);
            await store.set("games", Array.isArray(games) ? games : []);
            await store.save();
        } catch (err) {
            console.error("Failed to save settings on close:", err);
        }
        await appWindow.destroy();
    });
}

async function main(){
    noiseFrames = makeNoiseFrames();
    startBannerAnimationLoop();
    await onStart();

    const pages = [gameGridP1, gameGridP2, gameGridP3];

    arrowShow();
    clickAnimation();

    fileDir.addEventListener("click", async () => {
        await getGameInfo();
        await insertChannels();
        await attachListenersToGames();
    });

    dolphinDir.addEventListener("click", async () => {
        await getDolphinPath();
    })

    scrollTrack.addEventListener("wheel", (e) => {
        e.preventDefault();
    }, { passive: false });

    arrowLeft.addEventListener("click", () => {
        if (curPage > 1) {
            curPage--;
            scrollToPage(scrollTrack, pages[curPage - 1]);
            arrowShow();
        }
    });

    arrowRight.addEventListener("click", () => {
        if (curPage < 3) {
            curPage++;
            scrollToPage(scrollTrack, pages[curPage - 1]);
            arrowShow();
        }
    });

    window.addEventListener("keydown", async (e) => {
        const w = getCurrentWindow();

        if (e.key === "F11") {
            e.preventDefault();
            await w.setFullscreen(!(await w.isFullscreen()));
        }

        if (e.key === "Escape" && await w.isFullscreen()) {
            await w.setFullscreen(false);
        }
    });

    await onClose();
}

window.addEventListener("DOMContentLoaded", () => {
    main();
});