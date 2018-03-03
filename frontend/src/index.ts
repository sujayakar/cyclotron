import PIXI = require("pixi.js");
import Viewport = require("pixi-viewport");
import { SpanManager } from "./model";

export class Cyclotron {
    private spanManager;
    private app;

    private windowWidth;
    private windowHeight;
    private rectangles;
    private text;
    private ticker;
    private lanesDirty;
    private lastViewport;
    private timeline;
    private textOverlay;
    private bufferedMessages;

    constructor() {
        this.app = new PIXI.Application({
            antialias: false,
            transparent: false,
            resolution: window.devicePixelRatio,
        });

        this.windowWidth = window.innerWidth * 0.9;
        this.windowHeight = window.innerHeight * 0.9;

        this.app.renderer.backgroundColor = 0xfafafa;
        this.app.renderer.view.style.className = "viewport";
        this.app.renderer.autoResize = true;
        this.app.renderer.resize(this.windowWidth, this.windowHeight);
        document.body.appendChild(this.app.view);

        this.spanManager = new SpanManager();
        // TODO: Print that we're waiting for data or something here.
        var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
        this.bufferedMessages = [];
        var i = 0;
        socket.onmessage = event => {
            // setTimeout(() => { this.addEvent(JSON.parse(event.data)); }, i++ * 10);
            // this.bufferedMessages.push(JSON.parse(event.data));
            this.addEvent(JSON.parse(event.data));
        };
        socket.onopen = event => { socket.send("empty_file_release.log"); };
        socket.onerror = event => { alert(`Socket error ${event}`); };
        socket.onclose = event => { alert(`Socket closed ${event}`); };

        this.rectangles = {};
        this.text = {};
        this.timeline = new Viewport({
            screenWidth: this.windowWidth,
            screenHeight: this.windowHeight,
            worldWidth: 0,
            worldHeight: 0,
        });
        this.timeline.drag().wheel().decelerate();
        this.timeline.clamp({direction: "all"});
        this.timeline.clampZoom({});
        // Oh lord, monkey patch da zoom.
        this.timeline.fitHeight = function (height, center) {
            this.scale.y = this._screenHeight / height;
            return this;
        };
        this.app.stage.addChild(this.timeline);

        this.textOverlay = new PIXI.Container();
        this.textOverlay.x = 0;
        this.textOverlay.y = 0;
        this.textOverlay.width = this.windowWidth;
        this.textOverlay.height = this.windowHeight;
        this.app.stage.addChild(this.textOverlay);

        this.ticker = PIXI.ticker.shared;
        this.ticker.autoStart = true;
        this.ticker.add(this.draw, this);

        this.lanesDirty = false;
        this.lastViewport = {width: 0, height: 0, ts: 0};
    }

    private addEvent(event) {
        this.spanManager.addEvent(event);
        this.lanesDirty = true;
    }

    private viewportDirty() {
        let viewArea = this.timeline.hitArea;
        return this.lastViewport.width !== viewArea.width
            || this.lastViewport.height !== viewArea.height
            || this.lastViewport.ts !== viewArea.x;
    }

    private saveViewport() {
        let viewArea = this.timeline.hitArea;
        this.lastViewport = {
            width: viewArea.width,
            height: viewArea.height,
            ts: viewArea.x
        };
    }

    private draw() {
        if (this.lanesDirty) {
            this.lanesDirty = false;

            let maxHeight = this.spanManager.numLanes();
            if (maxHeight === 0 || this.spanManager.maxTime === 0) {
                return;
            }

            this.timeline.worldWidth = this.spanManager.maxTime;
            this.timeline.worldHeight = maxHeight;

            let clampZoom = this.timeline.plugins['clamp-zoom'];
            clampZoom.minHeight = maxHeight;
            clampZoom.maxHeight = maxHeight;
            clampZoom.maxWidth = this.spanManager.maxTime;

            let numDrawn = 0;
            this.spanManager.listLanes().forEach(lane => {
                lane.spans.forEach(span => {
                    let end = span.end ? span.end : this.spanManager.maxTime;
                    let rect = this.rectangles[span.id];
                    if (rect === undefined) {
                        rect = new PIXI.Graphics();
                        this.timeline.addChild(rect);
                        this.rectangles[span.id] = rect;
                    }
                    rect.clear();
                    rect.beginFill(0x484848);
                    rect.drawRect(
                        span.start,
                        lane.index,
                        end - span.start,
                        0.9,
                    );
                    rect.endFill();

                    numDrawn += 1;
                })
            });
            console.log(`Drew ${numDrawn} spans`);
        }

        if (this.viewportDirty()) {
            let maxHeight = this.spanManager.numLanes();
            if (maxHeight === 0 || this.spanManager.maxTime === 0) {
                return;
            }

            let startTs = this.timeline.hitArea.x;
            let endTs = startTs + this.timeline.hitArea.width;
            let laneHeightPx = this.windowHeight / maxHeight;
            let tsWidthPx = this.windowWidth / this.timeline.hitArea.width;

            let numLabels = 0;
            this.spanManager.listLanes().forEach(lane => {
                lane.spans.forEach(span => {
                    let visible = span.intersects(startTs, endTs);
                    let text = this.text[span.id];
                    if (text === undefined) {
                        let style = new PIXI.TextStyle({fill: "white"});
                        let name = span.name;
                        if (name.length > 80) {
                            name = name.substring(0, 77) + "...";
                        }
                        text = new PIXI.Text(name, style);
                        this.text[span.id] = text;
                        this.textOverlay.addChild(text);
                    }
                    text.visible = visible;

                    if (text.mask != null) {
                        text.mask.destroy();
                        text.mask = null;
                    }

                    if (!visible) {
                        return;
                    }
                    let scale = laneHeightPx / text.height;
                    let screenRelTs = span.start - this.timeline.hitArea.x;
                    if (screenRelTs < 0) {
                        screenRelTs = 0;
                    }
                    let end = (span.end ? span.end : this.spanManager.maxTime)
                        - this.timeline.hitArea.x;
                    if (end > this.timeline.hitArea.width) {
                        end = this.timeline.hitArea.width;
                    }

                    let widthTs = end - screenRelTs;

                    text.x = screenRelTs * tsWidthPx;
                    text.y = lane.index * laneHeightPx;
                    text.height = text.height * scale;
                    text.width = text.width * scale;

                    if (text.width * scale < 25) {
                        text.visible = false;
                        return;
                    }

                    if (text.width * scale > tsWidthPx * widthTs) {
                        let mask = new PIXI.Graphics();
                        mask.clear();
                        mask.beginFill(0x000000);
                        mask.drawRect(
                            text.x,
                            text.y,
                            tsWidthPx * widthTs,
                            text.height,
                        );
                        mask.endFill();
                        text.mask = mask;
                    }

                    numLabels++;
                });
            });
            console.log(`Drew ${numLabels} labels`);

            this.saveViewport();
        }
    }
}

window["cyclotron"] = new Cyclotron();
