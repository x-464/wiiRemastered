import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";

let games = [];

const fileDir = document.querySelector(".settingsBtnSVGLeft");

fileDir.addEventListener("click", async () => {

    const selected = await open({
        directory: true,
        multiple: false
    });
    if (!selected || Array.isArray(selected)) { return; }
    const entries = await readDir(selected);

    const isoPaths = entries
        .filter(entry => entry.isFile && entry.name?.toLowerCase().endsWith('.iso'))
        .map(entry => `${selected}${selected.endsWith('\\') || selected.endsWith('/') ? '' : '/'}${entry.name}`);
    
    for (const isoPath of isoPaths) {
        const id = await invoke('get_id', { path: isoPath });
        console.log(isoPath, id);

        // get title

        const bnrPath = await invoke("unwrap_iso", { isoPath: isoPath });
        console.log(bnrPath);

        const binPaths = await invoke("unwrap_bnr", { bnrPath });
        console.log(binPaths);

        let pngPaths = [];
        for (const binPath of binPaths) {
            if (binPath.bin_path.toLowerCase().endsWith("banner.bin") || binPath.bin_path.toLowerCase().endsWith("icon.bin")) {
                const bannerTpls = await invoke("unwrap_bin", { binPath: binPath.bin_path });

                for (const tpl of bannerTpls) {
                    const pngPath = await invoke("tpl_to_png", { tplPath: tpl.source_path });
                    pngPaths.push(pngPath);
                }
            }
        }

        const game = {
            id,
            title: "",
            isoPath,
            bnrPath,
            binPaths,
            pngPaths
        };
        console.log(game);
        games.push(game);
    }
});


const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");

function sendNullGame() {
    return `
    <div class="game">
        <svg class="gamePath" viewBox="0 0 200 113" xmlns="http://www.w3.org/2000/svg">
        
        <style>
            .channelWii{
                fill: #4a4a4a;
                font-family: "Roboto";
                font-weight: 650;
                font-size: 1.75rem;
                opacity: 5%;
            }
        </style>
        
        <defs>

            <pattern id="lineFill" width="4" height="5" patternUnits="userSpaceOnUse" patternTransform="translate(0,0)">
                <rect width="4" height="1" fill="rgb(190, 190, 190)" />
                <rect y="1" width="4" height="4" fill="rgb(167, 167, 167)" />

                <animateTransform
                    attributeName="patternTransform"
                    type="translate"
                    values="0,0; 0,-5;"
                    dur="1.5s"
                    repeatCount="indefinite" />
            </pattern>

            <filter id="tvStatic">
                <feTurbulence
                    type="fractalNoise"
                    baseFrequency="0.5"
                    numOctaves="1"
                    seed="2"
                    stitchTiles="stitch"
                    result="noise">

                    <animate
                        attributeName="seed"
                        values="1;12;24;36;48;60;72;84"
                        dur="0.18s"
                        repeatCount="indefinite" 
                    />
                </feTurbulence>

                <!-- make it grayscale -->
                <feColorMatrix in="noise" type="saturate" values="0" result="monoNoise" />

                <!-- crank contrast -->
                <feComponentTransfer in="monoNoise" result="staticNoise">
                    <feFuncR type="linear" slope="4" intercept="-1.5" />
                    <feFuncG type="linear" slope="4" intercept="-1.5" />
                    <feFuncB type="linear" slope="4" intercept="-1.5" />
                </feComponentTransfer>

                <!-- keep it only inside the fill shape -->
                <feComposite in="staticNoise" in2="SourceGraphic" operator="in" result="clippedNoise" />

                <!-- blend into your pattern -->
                <feBlend in="SourceGraphic" in2="clippedNoise" mode="screen" />
            </filter>
        </defs>

            <path transform="translate(5 6)" d="M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z" 
                fill="url(#lineFill)"
                filter="url(#tvStatic)"
                opacity="60%"
            />
            <path transform="translate(5 6)" d="M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z"
                fill="none"
                stroke="#bbbbbb"
                stroke-width="2"
            />

            <text class="channelWii">
                <tspan x="79" y="69">Wii</tspan>
            </text>

        </svg>
    </div>
    `;
}

for (let i = 0; i < 12; i++) {
    gameGridP1.insertAdjacentHTML("beforeend", sendNullGame());
    gameGridP2.insertAdjacentHTML("beforeend", sendNullGame());
    gameGridP3.insertAdjacentHTML("beforeend", sendNullGame());
}

const gamesGrid = document.querySelector(".gameGridWrapper");
const game = document.querySelector(".game");
const arrowLeft = document.querySelector(".arrowLeft");
const arrowRight = document.querySelector(".arrowRight");

gamesGrid.addEventListener("wheel", (e) => {
    e.preventDefault();
}, { passive: false });

let curPage = 1;

function arrowsShow() {
    if (curPage == 1) {
        arrowLeft.style.display = "none";
    }
    else {
        arrowLeft.style.display = "block";
    }
    if (curPage == 3) {
        arrowRight.style.display = "none";
    }
    else {
        arrowRight.style.display = "block";
    }
}
arrowsShow();

arrowLeft.addEventListener("click", () => {
    gamesGrid.scrollBy({ left: -(game.clientWidth * 4), behavior: "smooth" });

    curPage -= 1;
    arrowsShow();
});

arrowRight.addEventListener("click", () => {
    gamesGrid.scrollBy({ left: (game.clientWidth * 4), behavior: "smooth" });

    curPage += 1;
    arrowsShow();
});