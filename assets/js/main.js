import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";

const fileDir = document.querySelector(".settingsBtnSVGLeft");

fileDir.addEventListener("click", async () => {
    const selected = await open({
        directory: true,
        multiple: false
    });

    const files = await readDir(selected);
    // console.log(files);

    const firstIso = files.find(file => file.name?.toLowerCase().endsWith(".iso"));

    const fullPath = await join(selected, firstIso.name);

    console.log(fullPath);

    const result = await invoke("get_iso_metadata", {
        path: fullPath
    });

    console.log(result);
});


const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");

function sendGame(){
    return `
        <div class="game">
            <svg viewBox="0 0 200 113" xmlns="http://www.w3.org/2000/svg">
                <defs>
                    <clipPath id="gameClip">
                        <path transform="translate(5 6)" d="M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z" />
                    </clipPath>
                </defs>
            
                <path transform="translate(5 6)" d="M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z" 
                    fill="none"
                    stroke="#bbbbbb"
                    stroke-width="4"
                />

                <image 
                    href="assets/imgs/Untitled.jpg" 
                    x="0" 
                    y="0" 
                    width="200" 
                    height="111" 
                    preserveAspectRatio="xMidYMid slice"
                    clip-path="url(#gameClip)" 
                />
            </svg>
        </div>
    `;
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