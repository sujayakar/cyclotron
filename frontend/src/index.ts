import PIXI = require("pixi.js");
import Viewport = require("pixi-viewport");
import { SpanManager, Root } from "./model";

export class Cyclotron {
    private spanManager;
    private app;

    private windowWidth;
    private windowHeight;
    private rectangles;
    private ticker;
    private dirty;
    private timeline;

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
        socket.onmessage = event => { this.addEvent(JSON.parse(event.data)); };
        socket.onopen = event => { socket.send("empty_file_release.log"); };
        socket.onerror = event => { alert(`Socket error ${event}`); };
        socket.onclose = event => { alert(`Socket closed ${event}`); };

        this.rectangles = {};
        this.timeline = new Viewport({
            screenWidth: this.windowWidth,
            screenHeight: this.windowHeight,
            worldWidth: 0,
            worldHeight: 0,
        })
        this.app.stage.addChild(this.timeline);
        this.timeline.drag().pinch().decelerate();

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
        let {laneAssignment, visibleSpans, numLanes} = this.computeVisible(0, this.spanManager.maxTime);

        if (!this.dirty) {
            return;
        }
        this.dirty = false
;
        this.timeline.x = 0;
        this.timeline.y = 0;
        this.timeline.worldWidth = this.spanManager.maxTime;
        this.timeline.worldHeight = numLanes;

        console.log(`Drawing ${visibleSpans.length} spans`);

        visibleSpans.forEach(span => {
            let rectangle = this.rectangles[span.id];
            if (rectangle === undefined) {
                rectangle = new PIXI.Graphics();
                this.rectangles[span.id] = rectangle;
                this.timeline.addChild(rectangle);
            }
            let end = span.end ? span.end : this.spanManager.maxTime;
            rectangle.beginFill(0x66CCFF)
            rectangle.drawRect(
                span.start,
                laneAssignment[span.id],
                end - span.start,
                1,
                0.1,
            );
            rectangle.endFill();
        });

        this.timeline.scale.x = this.windowWidth / this.spanManager.maxTime;
        this.timeline.scale.y = this.windowHeight / numLanes;
    }

    private computeVisible(startTs, endTs) {
        let root = new Root(this.spanManager);
        let stack = root.overlappingChildren(startTs, endTs).reverse();

        var nextLane = 0;

        // lane -> max ts drawn
        var laneMaxTs = {}
        var laneAssignment = {};
        var visible = []

        var span;
        while (span = stack.pop()) {
            var lane = null;
            if (span.parent_id) {
                let parentLane = laneAssignment[span.parent_id];
                for (var candidate = parentLane + 1; candidate < nextLane; candidate++) {
                    let maxTs = laneMaxTs[candidate];
                    if (maxTs === undefined) {
                        throw new Error(`Missing max ts for ${lane}`);
                    }
                    if (maxTs <= span.start) {
                        lane = candidate;
                        break;
                    }
                }
            }
            if (lane === null) {
                lane = nextLane++;
            }

            laneMaxTs[lane] = span.end ? span.end : endTs;
            laneAssignment[span.id] = lane;
            visible.push(span);

            span.overlappingChildren(startTs, endTs).reverse().forEach(child => { stack.push(child); })
        }

        return {laneAssignment, visibleSpans: visible, numLanes: nextLane};
    }
}

window["cyclotron"] = new Cyclotron();
