import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { readTextFile } from '@tauri-apps/plugin-fs';
import { appDataDir, join } from "@tauri-apps/api/path";
import { convertFileSrc } from "@tauri-apps/api/core";
import { LazyStore } from "@tauri-apps/plugin-store";
import { getCurrentWindow } from "@tauri-apps/api/window";

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

    const selected = await open({
        directory: true,
        multiple: false
    });
    if (!selected || Array.isArray(selected)) { return; }
    gamesPath = await readDir(selected);

    const isoPaths = gamesPath
        .filter(entry => entry.isFile && entry.name?.toLowerCase().endsWith('.iso'))
        .map(entry => `${selected}${selected.endsWith('\\') || selected.endsWith('/') ? '' : '/'}${entry.name}`);

    for (const isoPath of isoPaths) {
        const id = await invoke('get_id', { path: isoPath });
        let title = await getTitle(id);

        const bnrPath = await invoke("unwrap_iso", { isoPath: isoPath });
        const binPaths = await invoke("unwrap_bnr", { bnrPath });
        
        let pngPaths = [];
        let pngPath = null;
        let jsonPath = null;

        for (const binPath of binPaths) {
            if (binPath.bin_path.toLowerCase().endsWith("banner.bin") || binPath.bin_path.toLowerCase().endsWith("icon.bin")) {
                const imgInfoPaths = await invoke("unwrap_bin", { binPath: binPath.bin_path });
                console.log(binPath.bin_path);
                for (const imgInfoPath of imgInfoPaths) {
                    if (imgInfoPath.source_path.toLowerCase().endsWith(".tpl") || imgInfoPath.source_path.toLowerCase().endsWith(".tex0")){
                        pngPath = await invoke("tpl_to_png", { tplPath: imgInfoPath.source_path, title: title });
                    }
                    
                    if (imgInfoPath.source_path.toLowerCase().endsWith("icon.brlyt")) {
                        jsonPath = await invoke("convert_brlyt", { brlytPath: imgInfoPath.source_path, title: title });
                    }
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
            pngPaths,
            jsonPath
        };
        games.push(game);
    }
}


async function getDolphinPath() {
    const selected = await open({
        multiple: false,
        filters: [
            {
                name: "Executable",
                extensions: ["exe"]
            }
        ]
    });

    if (!selected || Array.isArray(selected)) return;

    dolphinPath = selected;
}


async function makePane(pane, parentEl, gameNum) {
    const SVG_NS = "http://www.w3.org/2000/svg";

    if (!pane.visible) return;

    const group = document.createElementNS(SVG_NS, "g");
    group.setAttribute("transform", `translate(${pane.x}, ${pane.y}) scale(${pane.scale_x}, ${pane.scale_y})`);
    group.setAttribute("opacity", `${pane.alpha / 255}`);

    parentEl.appendChild(group);

    if (pane.type === "pic1" && pane.png_candidate){
        const img = document.createElementNS(SVG_NS, "image");
        
        const appDataPath = await appDataDir();
        const fullPngPath = await join(appDataPath, "generated_pngs", `${games[gameNum].title}/`, pane.png_candidate);
        const assetUrl = convertFileSrc(fullPngPath);
        img.setAttribute("href", assetUrl);
        img.setAttribute("x", `${-pane.width / 2}`);
        img.setAttribute("x", `${-pane.height / 2}`);

        // img.setAttribute("x", "0");
        // img.setAttribute("y", "0");

        img.setAttribute("width", `${pane.width}`);
        img.setAttribute("height", `${pane.height}`);
        img.setAttribute("preserveAspectRatio", "none");

        group.appendChild(img);
    }

    for (const child of pane.children || []){
        await makePane(child, group, gameNum);
    }
}


async function insertNullSVG(gameNum) {
    const nullSVG = await fetch("/assets/svgs/nullGame.svg");
    const nullText = await nullSVG.text();

    const wrapper = document.createElement("div");
    wrapper.setAttribute("class", `game ${gameNum}`);
    wrapper.innerHTML = nullText;

    return wrapper;
}


async function insertGameSVG(gameNum) {
    const res = await readTextFile(games[gameNum].jsonPath);
    let channelJson = JSON.parse(res);
    const SVG_NS = "http://www.w3.org/2000/svg";

    const wrapper = document.createElement("div");
    wrapper.setAttribute("class", `game ${gameNum}`);

    const gameSVG = document.createElementNS(SVG_NS, "svg");
    gameSVG.setAttribute("viewBox", "0 0 200 113");
    gameSVG.setAttribute("class", "gamePath");

    const preciseGroup = document.createElementNS(SVG_NS, "g");
    preciseGroup.setAttribute("class", "precisePath");
    gameSVG.appendChild(preciseGroup);

    for (const pane of channelJson.root) {
        console.log(pane);
        await makePane(pane, preciseGroup, gameNum);
    }

    wrapper.appendChild(gameSVG);

    const border = document.createElementNS(SVG_NS, "path");
    border.setAttribute("transform", "translate(5 6)");
    border.setAttribute("d", "M190,51.5c0-21.61-1.04-39.36-2.36-41.31-2.14-5.92-8.1-10.19-15.14-10.19H17.5C10.46,0,4.5,4.27,2.36,10.19,1.04,12.14,0,29.89,0,51.5s1.04,39.36,2.36,41.31c2.14,5.92,8.1,10.19,15.14,10.19h155c7.04,0,13-4.27,15.14-10.19,1.32-1.96,2.36-19.7,2.36-41.31Z");
    border.setAttribute("fill", "none");
    border.setAttribute("stroke", "#bbbbbb");
    border.setAttribute("stroke-width", "2");

    gameSVG.appendChild(border);

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
                gameGrids[i].appendChild(await insertGameSVG(gameIndex));
            }
            else {
                gameGrids[i].appendChild(await insertNullSVG(gameIndex));
            }
        }
    }

    for (const grid of gameGrids) {
        grid.style.visibility = "visible";
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


async function attachListenersToGames() {
    const gameCards = document.querySelectorAll(".game");
    const topWrapper = document.querySelector(".topWrapper");
    const gameTitleWrapper = document.querySelector(".gameTitleWrapper");
    const gameTitle = document.querySelector(".gameTitle");

    for (let i = 0; i < games.length; i++) {
        gameCards[i].addEventListener("mouseenter", () => {
            const gameCard = gameCards[i];
            const currentGrid = gameCard.closest(".gameGrid");

            const gameRect = gameCard.getBoundingClientRect();
            const wrapperRect = topWrapper.getBoundingClientRect();
            const gridRect = currentGrid.getBoundingClientRect();

            const x = gameRect.left - wrapperRect.left + gameRect.width / 2;
            const y = gameRect.top - wrapperRect.top + gameRect.height - 7;

            gameTitle.textContent = games[i].title;
            gameTitleWrapper.style.display = "flex";
            gameTitleWrapper.style.left = "0px";
            gameTitleWrapper.style.top = "0px";
            gameTitleWrapper.style.transform = `translate(${x}px, ${y}px) translateX(-50%)`;

            const titleRect = gameTitleWrapper.getBoundingClientRect();
            let correctedX = x;
            const gutter = 8;

            if (titleRect.left < gridRect.left + gutter) {
                correctedX += (gridRect.left + gutter) - titleRect.left;
            }

            if (titleRect.right > gridRect.right - gutter) {
                correctedX -= titleRect.right - (gridRect.right - gutter);
            }

            gameTitleWrapper.style.transform =
                `translate(${correctedX}px, ${y}px) translateX(-50%)`;
        });

        gameCards[i].addEventListener("mouseleave", () => {
            gameTitleWrapper.style.display = "none";
        });

        gameCards[i].addEventListener("click", async () => {
            const gameReturn = await invoke("open_game", { gamePath: games[i].isoPath, dolphinPath: String(dolphinPath) });
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

async function onStart() {
    dolphinPath = await store.get("dolphinPath");
    gamesPath = await store.get("gamesPath");
    games = await store.get("games");
    if (!Array.isArray(games)) games = [];
    console.log(games);

    await insertChannels();
}

async function onClose() {
    const appWindow = getCurrentWindow();

    await appWindow.onCloseRequested(async (event) => {
        await store.set("dolphinPath", dolphinPath);
        await store.set("gamesPath", gamesPath);
        await store.set("games", games);
        await store.save();
    });
}

async function main(){

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

    await onClose();
}

window.addEventListener("DOMContentLoaded", () => {
    main();
});