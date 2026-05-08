import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";

const fileDir = document.querySelector(".settingsBtnSVGLeft");

fileDir.addEventListener("click", async () => {
    // const selected = await open({
    //     directory: true,
    //     multiple: false
    // });

    // const files = await readDir(selected);
    // // console.log(files);

    // const firstIso = files.find(file => file.name?.toLowerCase().endsWith(".iso"));

    // const fullPath = await join(selected, firstIso.name);

    // console.log(fullPath);

    // const result = await invoke("get_iso_metadata", {
    //     path: fullPath
    // });

    // const debug = await invoke("get_bnr_from_iso", {
    //     path: fullPath
    // });
    
    // console.log(result);
    // console.log(debug);

    
    const selected = await open({
        directory: true,
        multiple: false
    });

    if (!selected || typeof selected !== "string") {
        console.log("Folder selection cancelled");
        return;
    }

    const extractResult = await invoke("extract_first_iso_banner", {
        folderPath: selected
    });

    console.log(extractResult);

});


const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");

function sendGame(){
    return `
    <div class="game">
        <svg class="gamePath" viewBox="0 0 200 113" xmlns="http://www.w3.org/2000/svg">
        
        <style>
            .channelWii{
                fill: #4a4a4a;
                font-family: "Roboto";
                font-weight: 650;
                font-size: 4cqmin;
                opacity: 5%;
                transform: translate(5cqw, 5cwh);
            }
        </style>
        
        <defs>
            <filter id="patternBlur" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="0.5" />
            </filter>

            <pattern id="lineFill" width="4" height="5" patternUnits="userSpaceOnUse" patternTransform="translate(0,0)">
                <g filter="url(#patternBlur)">
                    <rect width="4" height="1" fill="rgb(200, 200, 200)" />
                    <rect y="1" width="4" height="4" fill="rgb(181, 181, 181)" />
                </g>

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
                    baseFrequency="1.2"
                    numOctaves="1"
                    seed="2"
                    stitchTiles="stitch"
                    result="noise">
                    <animate
                    attributeName="seed"
                    values="1;12;24;36;48;60;72;84"
                    dur="0.18s"
                    repeatCount="indefinite" />
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
                
            />
            <path transform="translate(5 6)" d="M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z"
                fill="none"
                stroke="#bbbbbb"
                stroke-width="2"
            />

            <text class="channelWii">
                <tspan x="0" y="0">Wii</tspan>
            </text>

        </svg>
    </div>
    `;
    // fill = "url(#lineFill)"
    // filter = "url(#tvStatic)"
}

for (let i = 0; i < 12; i++) {
    gameGridP1.insertAdjacentHTML("beforeend", sendGame());
    gameGridP2.insertAdjacentHTML("beforeend", sendGame());
    gameGridP3.insertAdjacentHTML("beforeend", sendGame());
}

const games = document.querySelector(".gameGridWrapper");
const game = document.querySelector(".game");
const arrowLeft = document.querySelector(".arrowLeft");
const arrowRight = document.querySelector(".arrowRight");

games.addEventListener("wheel", (e) => {
    e.preventDefault();
}, { passive: false });

let curPage = 1;

function arrowsShow(){
    if (curPage == 1){
        arrowLeft.style.display = "none";
    }
    else{
        arrowLeft.style.display = "block";
    }
    if(curPage == 3){
        arrowRight.style.display = "none";
    }
    else{
        arrowRight.style.display = "block";
    }
}
arrowsShow();

arrowLeft.addEventListener("click", () => {
    games.scrollBy({ left: -(game.clientWidth * 4), behavior: "smooth" });

    curPage -= 1;
    arrowsShow();
});

arrowRight.addEventListener("click", () => {
    games.scrollBy({ left: (game.clientWidth * 4), behavior: "smooth" });

    curPage += 1;
    arrowsShow();
});