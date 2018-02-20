import PIXI = require("pixi.js");
import Viewport = require("pixi-viewport");
import { SpanManager } from "./model";

export class Cyclotron {
    private spanManager;
    private app;

    private windowWidth;
    private windowHeight;
    private rectangles;
    private ticker;
    private dirty;
    private timeline;
    private bufferedMessages;

    constructor() {
        this.app = new PIXI.Application({
            antialias: false,
            transparent: false,
            resolution: window.devicePixelRatio,
        });

        this.windowWidth = window.innerWidth * 0.9;
        this.windowHeight = window.innerHeight * 0.9;

        this.app.renderer.backgroundColor = 0x061639;
        this.app.renderer.view.style.className = "viewport";
        this.app.renderer.autoResize = true;
        this.app.renderer.resize(this.windowWidth, this.windowHeight);
        document.body.appendChild(this.app.view);

        this.spanManager = new SpanManager();
        // TODO: Print that we're waiting for data or something here.
        var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
        this.bufferedMessages = [];
        socket.onmessage = event => {
            // this.bufferedMessages.push(JSON.parse(event.data));
            // if (i++ > 1000) {
            //     return;
            // }
            this.addEvent(JSON.parse(event.data));
        };
        socket.onopen = event => { socket.send("empty_file_release.log"); };
        socket.onerror = event => { alert(`Socket error ${event}`); };
        socket.onclose = event => { alert(`Socket closed ${event}`); };

        this.rectangles = {};
        this.timeline = new Viewport({
            screenWidth: this.windowWidth,
            screenHeight: this.windowHeight,
            worldWidth: 0,
            worldHeight: 0,
        });
        this.timeline.drag().wheel().decelerate();
        this.timeline.clamp({direction: "all"});

        this.app.stage.addChild(this.timeline);

        this.ticker = PIXI.ticker.shared;
        this.ticker.autoStart = true;
        this.ticker.add(this.drawDemo, this);

        this.dirty = false;
    }

    private addEvent(event) {
        this.spanManager.addEvent(event);
        this.dirty = true;
    }

    private drawDemo() {
        if (!this.dirty) {
            return;
        }
        this.dirty = false;

        this.timeline.x = 0;
        this.timeline.y = 0;

        let maxHeight = this.spanManager.numLanes();
        if (maxHeight === 0 || this.spanManager.maxTime === 0) {
            return;
        }

        this.timeline.worldWidth = this.spanManager.maxTime;
        this.timeline.worldHeight = maxHeight;
        this.timeline.scale.x = this.windowWidth / this.spanManager.maxTime;
        this.timeline.scale.y = this.windowHeight / maxHeight;

        let numDrawn = 0;
        this.spanManager.listLanes().forEach(lane => {
            lane.spans.forEach(span => {
                let end = span.end ? span.end : this.spanManager.maxTime;
                let container = this.rectangles[span.id];
                if (container === undefined) {
                    container = new PIXI.Container();
                    let rect = new PIXI.Graphics();
                    rect.beginFill(0x66CCFF)
                    rect.drawRect(
                        0,
                        0,
                        end - span.start,
                        0.9,
                    );
                    rect.endFill();
                    container.addChild(rect);

                    let name = new PIXI.Text(span.name);
                    name.width = end - span.start;
                    name.height = 0.9;
                    container.addChild(name);

                    this.rectangles[span.id] = container;
                    this.timeline.addChild(container);
                }

                let rect = container.children[0];
                rect.width = end - span.start;
                rect.height = 0.9;

                // console.log(`Rect ${span.name}: ${container.children[0].width} x ${container.children[0].height}`)
                // console.log(`Text ${span.name}: ${container.children[1].width} x ${container.children[1].height}`)
                // let rect = container.children[0];
                // rect.beginFill(0x66CCFF);
                // rect.drawRect(0, 0, end - span.start, 0.9);
                // rect.endFill();
                // rect.width = end - span.start;
                // rect.height = 1;

                // console.log(`${span.name} -> (${span.start}, ${lane.index}, ${end - span.start}, 0.9)`);

                container.x = span.start;
                container.y = lane.index;
                container.width = end - span.start;
                container.height = 0.9;

                numDrawn += 1;
            })
        });
        console.log(`Drew ${numDrawn} spans`);
    }
}

window["cyclotron"] = new Cyclotron();
