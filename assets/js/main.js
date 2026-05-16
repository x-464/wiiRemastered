import { open } from "@tauri-apps/plugin-dialog";
import { readDir } from "@tauri-apps/plugin-fs";
import { readFile } from "@tauri-apps/plugin-fs";
import { invoke } from "@tauri-apps/api/core";
import { readTextFile } from '@tauri-apps/plugin-fs';
import { appDataDir, join } from "@tauri-apps/api/path";
import { convertFileSrc } from "@tauri-apps/api/core";

let games = [];
let amountOfGames = 0;
let curPage = 1;

const fileDir = document.querySelector(".settingsBtnSVGLeft");
const scrollTrack = document.querySelector(".scrollTrack");
const arrowLeft = document.querySelector(".arrowWrapL");
const arrowRight = document.querySelector(".arrowWrapR");

const gameGridP1 = document.querySelector(".gameGrid.P1");
const gameGridP2 = document.querySelector(".gameGrid.P2");
const gameGridP3 = document.querySelector(".gameGrid.P3");


async function getGameInfo() {
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
        let title = await getTitle(id);

        const bnrPath = await invoke("unwrap_iso", { isoPath: isoPath });
        const binPaths = await invoke("unwrap_bnr", { bnrPath });

        let pngPaths = [];
        let pngPath = null;
        let jsonPath = null;

        for (const binPath of binPaths) {
            if (binPath.bin_path.toLowerCase().endsWith("banner.bin") || binPath.bin_path.toLowerCase().endsWith("icon.bin")) {
                const imgInfoPaths = await invoke("unwrap_bin", { binPath: binPath.bin_path });

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
        amountOfGames++;
    }

    await insertChannels();
    attachListenersForTitles();
}


async function insertNullSVG() {
    const nullSVG = await fetch("/assets/svgs/nullGame.svg");
    const nullText = await nullSVG.text();

    const wrapper = document.createElement("div");
    wrapper.className = "game";
    wrapper.innerHTML = nullText;

    return wrapper;
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
        // img.setAttribute("x", `${-pane.width / 2}`);
        // img.setAttribute("x", `${-pane.height / 2}`);

        img.setAttribute("x", "0");
        img.setAttribute("y", "0");

        img.setAttribute("width", `${pane.width}`);
        img.setAttribute("height", `${pane.height}`);
        img.setAttribute("preserveAspectRatio", "none");

        group.appendChild(img);
    }

    for (const child of pane.children || []){
        await makePane(child, group, gameNum);
    }
}

async function insertGameSVG(gameNum) {
    const res = await readTextFile(games[gameNum].jsonPath);
    let channelJson = JSON.parse(res);
    const SVG_NS = "http://www.w3.org/2000/svg";

    const wrapper = document.createElement("div");
    wrapper.className = "game";

    const gameSVG = document.createElementNS(SVG_NS, "svg");
    gameSVG.setAttribute("viewBox", "0 0 200 113");
    gameSVG.setAttribute("class", "gamePath");

    const preciseGroup = document.createElementNS(SVG_NS, "g");
    preciseGroup.setAttribute("class", "precisePath");
    gameSVG.appendChild(preciseGroup);

    for (const pane of channelJson.root) {
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

    for (let i = 0; i < 3; i++) {
        for (let j = 0; j < 12; j++) {
            const gameIndex = i * 12 + j;

            if (j < spacesTaken[i]) {
                gameGrids[i].appendChild(await insertGameSVG(gameIndex));
            }
            else {
                gameGrids[i].appendChild(await insertNullSVG());
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

function attachListenersForTitles() {
    const gameElements = document.querySelectorAll(".precisePath");
    const gameGrid = document.querySelector(".gameGrid.P1");
    const topWrapper = document.querySelector(".topWrapper");
    const gameTitleWrapper = document.querySelector(".gameTitleWrapper");
    const gameTitle = document.querySelector(".gameTitle");

    for (let i = 0; i < amountOfGames; i++){
        // gameElements.forEach(game => {
            gameElements[i].addEventListener("mouseenter", () => {
                const gameRect = gameElements[i].getBoundingClientRect();
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

                gameTitle.innerHTML = `${games[i].title}`;
            })
            gameElements[i].addEventListener("mouseleave", () => {
                gameTitleWrapper.style.display = "none";
            })
        // })
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


async function main(){
    const pages = [gameGridP1, gameGridP2, gameGridP3];

    arrowShow();
    await insertChannels();
    clickAnimation();

    fileDir.addEventListener("click", async () => {
        await getGameInfo();
    });

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
}
main();