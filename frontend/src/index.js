"use strict";
exports.__esModule = true;
var PIXI = require("pixi.js");
var Viewport = require("pixi-viewport");
var model_1 = require("./model");
var Cyclotron = /** @class */ (function () {
    function Cyclotron() {
        var _this = this;
        this.app = new PIXI.Application({
            antialias: false,
            transparent: false,
            resolution: window.devicePixelRatio
        });
        this.windowWidth = window.innerWidth * 0.9;
        this.windowHeight = window.innerHeight * 0.9;
        this.app.renderer.backgroundColor = 0xfafafa;
        this.app.renderer.view.style.className = "viewport";
        this.app.renderer.autoResize = true;
        this.app.renderer.resize(this.windowWidth, this.windowHeight);
        document.body.appendChild(this.app.view);
        this.spanManager = new model_1.SpanManager();
        // TODO: Print that we're waiting for data or something here.
        var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
        this.bufferedMessages = [];
        var i = 0;
        socket.onmessage = function (event) {
            // setTimeout(() => { this.addEvent(JSON.parse(event.data)); }, i++ * 10);
            // this.bufferedMessages.push(JSON.parse(event.data));
            _this.addEvent(JSON.parse(event.data));
        };
        socket.onopen = function (event) { socket.send("empty_file_release.log"); };
        socket.onerror = function (event) { alert("Socket error " + event); };
        socket.onclose = function (event) { alert("Socket closed " + event); };
        this.rectangles = {};
        this.text = {};
        this.timeline = new Viewport({
            screenWidth: this.windowWidth,
            screenHeight: this.windowHeight,
            worldWidth: 0,
            worldHeight: 0
        });
        this.timeline.drag().wheel().decelerate();
        this.timeline.clamp({ direction: "all" });
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
        this.lastViewport = { width: 0, height: 0, ts: 0 };
    }
    Cyclotron.prototype.addEvent = function (event) {
        this.spanManager.addEvent(event);
        this.lanesDirty = true;
    };
    Cyclotron.prototype.viewportDirty = function () {
        var viewArea = this.timeline.hitArea;
        return this.lastViewport.width !== viewArea.width
            || this.lastViewport.height !== viewArea.height
            || this.lastViewport.ts !== viewArea.x;
    };
    Cyclotron.prototype.saveViewport = function () {
        var viewArea = this.timeline.hitArea;
        this.lastViewport = {
            width: viewArea.width,
            height: viewArea.height,
            ts: viewArea.x
        };
    };
    Cyclotron.prototype.draw = function () {
        var _this = this;
        if (this.lanesDirty) {
            this.lanesDirty = false;
            var maxHeight = this.spanManager.numLanes();
            if (maxHeight === 0 || this.spanManager.maxTime === 0) {
                return;
            }
            this.timeline.worldWidth = this.spanManager.maxTime;
            this.timeline.worldHeight = maxHeight;
            var clampZoom = this.timeline.plugins['clamp-zoom'];
            clampZoom.minHeight = maxHeight;
            clampZoom.maxHeight = maxHeight;
            clampZoom.maxWidth = this.spanManager.maxTime;
            var numDrawn_1 = 0;
            this.spanManager.listLanes().forEach(function (lane) {
                lane.spans.forEach(function (span) {
                    var end = span.end ? span.end : _this.spanManager.maxTime;
                    var rect = _this.rectangles[span.id];
                    if (rect === undefined) {
                        rect = new PIXI.Graphics();
                        _this.timeline.addChild(rect);
                        _this.rectangles[span.id] = rect;
                    }
                    rect.clear();
                    rect.beginFill(0x484848);
                    rect.drawRect(span.start, lane.index, end - span.start, 0.9);
                    rect.endFill();
                    numDrawn_1 += 1;
                });
            });
            console.log("Drew " + numDrawn_1 + " spans");
        }
        if (this.viewportDirty()) {
            var maxHeight = this.spanManager.numLanes();
            if (maxHeight === 0 || this.spanManager.maxTime === 0) {
                return;
            }
            var startTs_1 = this.timeline.hitArea.x;
            var endTs_1 = startTs_1 + this.timeline.hitArea.width;
            var laneHeightPx_1 = this.windowHeight / maxHeight;
            var tsWidthPx_1 = this.windowWidth / this.timeline.hitArea.width;
            var numLabels_1 = 0;
            this.spanManager.listLanes().forEach(function (lane) {
                lane.spans.forEach(function (span) {
                    var visible = span.intersects(startTs_1, endTs_1);
                    var text = _this.text[span.id];
                    if (text === undefined) {
                        var style = new PIXI.TextStyle({ fill: "white" });
                        text = new PIXI.Text(span.name, style);
                        _this.text[span.id] = text;
                        _this.textOverlay.addChild(text);
                    }
                    text.visible = visible;
                    text.mask = null;
                    if (!visible) {
                        return;
                    }
                    var scale = laneHeightPx_1 / text.height;
                    var screenRelTs = span.start - _this.timeline.hitArea.x;
                    if (screenRelTs < 0) {
                        screenRelTs = 0;
                    }
                    var end = (span.end ? span.end : _this.spanManager.maxTime)
                        - _this.timeline.hitArea.x;
                    if (end > _this.timeline.hitArea.width) {
                        end = _this.timeline.hitArea.width;
                    }
                    var widthTs = end - screenRelTs;
                    text.x = screenRelTs * tsWidthPx_1;
                    text.y = lane.index * laneHeightPx_1;
                    text.height = text.height * scale;
                    text.width = text.width * scale;
                    if (tsWidthPx_1 < 25) {
                        text.visible = false;
                        return;
                    }
                    if (text.width * scale > tsWidthPx_1 * widthTs) {
                        text.visible = false;
                        return;
                    }
                    numLabels_1++;
                });
            });
            console.log("Drew " + numLabels_1 + " labels");
            this.saveViewport();
        }
    };
    return Cyclotron;
}());
exports.Cyclotron = Cyclotron;
window["cyclotron"] = new Cyclotron();
