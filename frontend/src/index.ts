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

        this.axis.call(d3.axisBottom(axisScale).ticks(10).tickFormat(seconds => {
            let delta = seconds - startTs;
            function formatTime(n) {
                if (delta < 0.000001) {
                    return `${(n * 1e9).toFixed(0)}ns`;
                }
                else if (delta < 0.001) {
                    return `${(n * 1e6).toFixed(0)}μs`;
                } else if (delta < 1) {
                    return `${(n * 1e3).toFixed(0)}ms`;
                } else if (delta < 60) {
                    return `${n.toFixed(0)}s`;
                } else {
                    return `${(n / 60).toFixed(0)}m`;
                }
            }
            return formatTime(seconds);
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
    private ticker;
    private lanesDirty;
    private lastViewport;
    private timeline;
    private textOverlay;
    private arrowOverlay;

    private arrowColor;

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

        this.arrowOverlay = new PIXI.Container();
        this.arrowOverlay.x = 0;
        this.arrowOverlay.y = 0;
        this.arrowOverlay.width = this.windowWidth;
        this.arrowOverlay.height = this.viewportHeight;
        this.app.stage.addChild(this.arrowOverlay);
        this.arrowColor = 0xca271b;

        this.ticker = PIXI.ticker.shared;
        this.ticker.autoStart = true;
        this.ticker.add(this.draw, this);

        this.lastViewport = {width: 0, height: 0, ts: 0};

        this.spanManager = new SpanManager(this.timeline);
        // TODO: Print that we're waiting for data or something here.
        var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
        var i = 0;
        socket.onmessage = event => { this.addEvent(JSON.parse(event.data)); };
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
        this.drawArrowOverlay(startTs, endTs, laneHeightPx, tsWidthPx, assignment);

        this.saveViewport();
    }

    private drawTextOverlay(startTs, endTs, laneHeightPx, tsWidthPx, assignment) {
        this.textOverlay.removeChildren();

        this.spanManager.listLanes().forEach(lane => {
            lane.spans.forEach(span => {
                let text = span.text;
                let visible = span.overlaps(startTs, endTs);

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

                this.textOverlay.addChild(text);
            });
        });
    }

    private drawArrowOverlay(startTs, endTs, laneHeightPx, tsWidthPx, assignment) {
        this.arrowOverlay.removeChildren();

        this.spanManager.wakeups.forEach(wakeup => {
            if (!wakeup.end_ts) {
                return;
            }
            let waking = this.spanManager.getSpan(wakeup.waking_id);
            if (!waking.overlaps(startTs, endTs)) {
                return;
            }
            if (assignment[waking.laneID] === undefined) {
                throw new Error(`Missing assignment for ${waking.laneID}`);
            }

            let parked = this.spanManager.getSpan(wakeup.parked_id);
            if (!parked.overlaps(startTs, endTs)) {
                return;
            }
            if (assignment[parked.laneID] === undefined) {
                throw new Error(`Missing assignment for ${parked.laneID}`);
            }

            let x1 = (wakeup.start_ts - this.timeline.hitArea.x) * tsWidthPx;
            let y1 = (assignment[waking.laneID] + 0.5) * laneHeightPx;
            let x2 = (wakeup.end_ts - this.timeline.hitArea.x) * tsWidthPx;
            let y2 = (assignment[parked.laneID] + 0.5) * laneHeightPx;

            let cX1 = x1;
            let cY1 = 0.8 * y1 + 0.2 * y2;
            let cX2 = x1;
            let cY2 = y2;

            let arrow = wakeup.arrow;
            arrow.clear();
            arrow.lineStyle(1.5, this.arrowColor, 0.5);
            arrow.moveTo(x1, y1);
            arrow.bezierCurveTo(cX1, cY1, cX2, cY2, x2, y2);

            let arrowSize = 5;
            if (x2 - x1 < 20) {
                arrowSize = 0.25 * (x2 - x1);
            }
            // Draw the arrow head
            arrow.beginFill(this.arrowColor, 0.5);
            arrow.drawPolygon([x2, y2,
                               x2-arrowSize, y2+arrowSize/2,
                               x2-arrowSize, y2-arrowSize/2]);
            arrow.endFill();

            this.arrowOverlay.addChild(arrow);
        })
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
