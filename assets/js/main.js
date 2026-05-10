import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";

let games = [];
let amountOfGames = 0;

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
        amountOfGames++;
    }
    console.log(amountOfGames);

    let gameAmount = amountOfGames;
    const gameGrids = [gameGridP1, gameGridP2, gameGridP3];
    const spacesTaken = [12, 12, 12];
    const maxGames = 36;
    const takenGames = maxGames - amountOfGames;

    const page1 = 12 - gameAmount;
    gameAmount - page1;
    const page2 = 12 - (gameAmount - page1);
    gameAmount - page2;
    const page3 = 12 - (gameAmount - page2);
    gameAmount - page3;
    console.log(gameAmount, page1, page2, page3);
});


const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");

async function makeNullGames() {
    const res = await fetch("/assets/svgs/nullGame.svg");
    const svgText = await res.text();
    const svg = `<div class="game">${svgText}</div>`;
    
    const gameGrids = [gameGridP1, gameGridP2, gameGridP3];
    const spacesTaken = [12, 12, 12];

    for (let j = 0; j < 3; j++){
        for (let i = 0; i < spacesTaken[j]; i++){
            gameGrids[j].style.backgroundColor = "black";
            spacesTaken[j]--;
        }
    }
    

    for (let j = 0; j < 3; j++){
        for (let i = 0; i < spacesTaken[j]; i++) {
            gameGrids[j].insertAdjacentHTML("beforeend", svg);
        }
    }
}
makeNullGames();



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

function scrollPageToCenter(container, page) {
    const target =
        page.offsetLeft -
        (container.clientWidth / 2) +
        (page.clientWidth / 2);

    container.scrollTo({
        left: target,
        behavior: "smooth"
    });
}

const pages = [gameGridP1, gameGridP2, gameGridP3];

arrowLeft.addEventListener("click", () => {
    if (curPage > 1) {
        curPage--;
        scrollPageToCenter(gamesGrid, pages[curPage - 1]);
        arrowsShow();
    }
});

arrowRight.addEventListener("click", () => {
    if (curPage < pages.length) {
        curPage++;
        scrollPageToCenter(gamesGrid, pages[curPage - 1]);
        arrowsShow();
    }
});