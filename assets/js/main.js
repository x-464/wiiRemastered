import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { join } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";

let games = [];
let amountOfGames = 0;
let curPage = 1;

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

        let title = await getTitle(id);

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
            title,
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

    makeGames();
});


const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");

async function insertSVGs() {
    const nullSVG = await fetch("/assets/svgs/nullGame.svg");
    const nullText = await nullSVG.text();
    const nullGame = `<div class="game">${nullText}</div>`;

    const realSVG = await fetch("/assets/svgs/realGame.svg");
    const realText = await realSVG.text();
    const realGame = `<div class="game">${realText}</div>`;

    let remainingGames = amountOfGames;
    const gameGrids = [gameGridP1, gameGridP2, gameGridP3];

    const page1Games = Math.min(remainingGames, 12);
    remainingGames -= page1Games;
    const page2Games = Math.min(remainingGames, 12);
    remainingGames -= page2Games;
    const page3Games = Math.min(remainingGames, 12);
    remainingGames -= page3Games;

    const spacesTaken = [page1Games, page2Games, page3Games];

    gameGridP1.innerHTML = "";
    gameGridP2.innerHTML = "";
    gameGridP3.innerHTML = "";

    for (let j = 0; j < 3; j++) {
        for (let i = 0; i < 12; i++) {
            if (i < spacesTaken[j]) {
                gameGrids[j].insertAdjacentHTML("beforeend", realGame);
            }
            else {
                gameGrids[j].insertAdjacentHTML("beforeend", nullGame);
            }
        }
    }
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


const scrollTrack = document.querySelector(".scrollTrack");
const arrowLeft = document.querySelector(".arrowWrapL");
const arrowRight = document.querySelector(".arrowWrapR");

scrollTrack.addEventListener("wheel", (e) => {
    e.preventDefault();
}, { passive: false });

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
arrowShow();

const pages = [gameGridP1, gameGridP2, gameGridP3];

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
})


function attachListenersForTitles(){
    const gameElements = document.querySelectorAll(".precisePath");
    const gameTitleWrapper = document.querySelector(".gameTitleWrapper");
    const gameTitle = document.querySelector(".gameTitle");
    const gameGrid = document.querySelector(".gameGrid.P1");
    const topWrapper = document.querySelector(".topWrapper");

    gameElements.forEach(game => {
        game.addEventListener("mouseenter", () => {
            const gameRect = game.getBoundingClientRect();
            const wrapperRect = topWrapper.getBoundingClientRect();

            const currentGameX = (gameRect.left - wrapperRect.left) + (gameRect.width / 2);
            const currentGameY = gameRect.bottom - wrapperRect.top;

            gameTitleWrapper.style.display = "flex";

            gameTitleWrapper.style.top = `${currentGameY + 10}px`;
            gameTitleWrapper.style.left = `${currentGameX}px`;

            const x = gameRect.left - wrapperRect.left + gameRect.width / 2;
            const y = gameRect.top - wrapperRect.top + gameRect.height + 10;

            gameTitleWrapper.style.display = "flex";
            gameTitleWrapper.style.left = "0px";
            gameTitleWrapper.style.top = "0px";
            gameTitleWrapper.style.transform = `translate(${x}px, ${y}px) translateX(-50%)`;
        })
        game.addEventListener("mouseleave", () => {
            gameTitleWrapper.style.display = "none";
        })
    })
}

async function makeGames(){
    await insertSVGs();
    attachListenersForTitles();
}
makeGames();