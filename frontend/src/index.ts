import PIXI = require("pixi.js");
import Viewport = require("pixi-viewport");
import { SpanManager } from "./model";
import d3 = require("d3");

class Axis {
    private container;
    private axis;

    constructor(private windowWidth, private axisHeight) {
        this.container = d3.select("body")
            .append("svg")
            .attr("width", windowWidth)
            .attr("height", axisHeight)
            .attr("class", "chart");
        this.axis = this.container.append("g")
            .attr("width", windowWidth)
            .attr("class", "top-axis")
            .append("g");

    }

    public update(startTs, endTs) {
        let axisScale = d3.scaleLinear()
            .domain([startTs, endTs])
            .range([0, this.windowWidth]);

        this.axis.call(d3.axisBottom(axisScale).ticks(5).tickFormat(seconds => {
            let delta = seconds - startTs;
            function formatTime(n, precision) {
                if (n < 0.001) {
                    return `${(n * 1e6).toFixed(precision)}μs`;
                } else if (n < 1) {
                    return `${(n * 1e3).toFixed(precision)}ms`;
                } else if (n < 60) {
                    return `${n.toFixed(precision)}s`;
                } else {
                    return `${(n / 60).toFixed(precision)}m`;
                }
            }
            return `${formatTime(startTs, 0)}+${formatTime(delta, 2)}`;
        }));
    }
}

export class Cyclotron {
    private spanManager;
    private app;
    private axis;

    private windowWidth;
    private windowHeight;
    private viewportHeight;
    private text;
    private ticker;
    private lanesDirty;
    private lastViewport;
    private timeline;
    private textOverlay;
    private bufferedMessages;

    constructor() {
        this.windowWidth = window.innerWidth * 0.9;
        this.windowHeight = window.innerHeight * 0.9;

        let axisHeight = this.windowHeight * 0.05;
        this.viewportHeight = this.windowHeight * 0.95;

        this.axis = new Axis(this.windowWidth, axisHeight);

        this.app = new PIXI.Application({
            antialias: false,
            transparent: false,
            resolution: window.devicePixelRatio,
        });
        this.app.renderer.backgroundColor = 0xfafafa;
        this.app.renderer.view.style.className = "viewport";
        this.app.renderer.autoResize = true;
        this.app.renderer.resize(this.windowWidth, this.viewportHeight);
        document.body.appendChild(this.app.view);

        this.text = {};
        this.timeline = new Viewport({
            screenWidth: this.windowWidth,
            screenHeight: this.viewportHeight,
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
        this.textOverlay.height = this.viewportHeight;
        this.app.stage.addChild(this.textOverlay);

        this.ticker = PIXI.ticker.shared;
        this.ticker.autoStart = true;
        this.ticker.add(this.draw, this);

        this.lastViewport = {width: 0, height: 0, ts: 0};

        this.spanManager = new SpanManager(this.timeline);
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
    }

    private addEvent(event) {
        this.spanManager.addEvent(event);
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

    private drawLanes(assignment) {
        this.drawVisibleLanes(assignment);
        this.spanManager.dirty = false;
    }

    private computeAssignment(startTs, endTs) {
        let nextLane = 0;
        let assignment = {};
        this.spanManager.listLanes().forEach(lane => {
            lane.updateMaxTs(this.spanManager.maxTime);
            if (lane.overlaps(startTs, endTs)) {
                assignment[lane.id] = nextLane++;

            }
        });
        return assignment;
    }

    private drawVisibleLanes(assignment) {
        let numLanes = Object.keys(assignment).length;

        this.timeline.worldWidth = this.spanManager.maxTime;
        this.timeline.worldHeight = numLanes;

        let clampZoom = this.timeline.plugins['clamp-zoom'];
        clampZoom.minHeight = numLanes;
        clampZoom.maxHeight = numLanes;
        clampZoom.maxWidth = this.spanManager.maxTime;

        this.spanManager.listLanes().forEach(lane => {
            lane.container.x = 0;
            let offset = assignment[lane.id];
            if (offset === undefined) {
                lane.container.visible = false;
            } else {
                lane.container.visible = true;
                lane.container.y = offset;
            }
        })
    }

    private drawViewport(assignment) {
        let startTs = this.timeline.hitArea.x;
        let endTs = startTs + this.timeline.hitArea.width;
        this.axis.update(startTs, endTs);

        let maxHeight = Object.keys(assignment).length;
        let laneHeightPx = this.viewportHeight / maxHeight;
        let tsWidthPx = this.windowWidth / this.timeline.hitArea.width;
        this.drawTextOverlay(startTs, endTs, laneHeightPx, tsWidthPx, assignment);

        this.saveViewport();
    }

    private drawTextOverlay(startTs, endTs, laneHeightPx, tsWidthPx, assignment) {
        let numLabels = 0;
        this.spanManager.listLanes().forEach(lane => {
            lane.spans.forEach(span => {
                let visible = span.overlaps(startTs, endTs);
                let text = this.text[span.id];
                if (text === undefined) {
                    let style = new PIXI.TextStyle({fill: "white"});
                    text = new PIXI.Text(span.name, style);
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

                if (assignment[lane.id] === undefined) {
                    throw new Error(`Missing assignment for ${lane.id}`);
                }

                text.x = screenRelTs * tsWidthPx;
                text.y = assignment[lane.id] * laneHeightPx;
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
    }

    private draw() {
        if (this.spanManager.numLanes() === 0 || this.spanManager.maxTime === 0) {
            return;
        }
        if (!this.viewportDirty() && !this.spanManager.dirty) {
            return;
        }

        let startTs = this.timeline.hitArea.x;
        let endTs = startTs + this.timeline.hitArea.width;
        let assignment = this.computeAssignment(startTs, endTs);
        this.drawLanes(assignment);
        this.drawViewport(assignment);

        // TODO: Why do we need this second draw to get text/rects to line up?
        this.drawLanes(assignment);
    }
}

window["cyclotron"] = new Cyclotron();
