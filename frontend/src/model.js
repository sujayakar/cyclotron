"use strict";
exports.__esModule = true;
var lane_1 = require("./lane");
var OnCPU = /** @class */ (function () {
    function OnCPU(start) {
        this.start = start;
        this.end = null;
    }
    OnCPU.prototype.isOpen = function () {
        return this.end === null;
    };
    OnCPU.prototype.close = function (ts) {
        if (!this.isOpen()) {
            throw new Error("Double close on trace");
        }
        if (this.start > ts) {
            throw new Error("Trace wth start after end");
        }
        this.end = ts;
    };
    return OnCPU;
}());
var Span = /** @class */ (function () {
    function Span(name, id, parent_id, start, metadata, threadName) {
        var _this = this;
        this.name = name;
        this.id = id;
        this.parent_id = parent_id;
        this.start = start;
        this.metadata = metadata;
        this.threadName = threadName;
        this.toString = function () {
            return "Span(id: " + _this.id + ", parent_id: " + _this.parent_id + ", name: " + _this.name + ", start: " + _this.start + ", end: " + _this.end + ")";
        };
        this.end = null;
        this.scheduled = [];
        this.outcome = null;
        this.expanded = true;
        this.laneID = null;
        this.freeLanes = {};
        this.maxSubtreeLaneID = null;
    }
    Span.prototype.isOpen = function () {
        return this.end === null;
    };
    Span.prototype.intersects = function (start, end) {
        return this.start < end && (this.end === null || this.end > start);
    };
    Span.prototype.overlaps = function (span) {
        var first = this.start < span.start ? this : span;
        var second = this.start < span.start ? span : this;
        return first.end === null || second.start < first.end;
    };
    Span.prototype.close = function (ts) {
        if (!this.isOpen()) {
            throw new Error("Double close on span " + this.id);
        }
        if (this.scheduled.length > 0) {
            if (this.scheduled[this.scheduled.length - 1].isOpen()) {
                throw new Error("Closing with open trace for " + this.id);
            }
        }
        if (this.start > ts) {
            throw new Error("Span with start after end " + this.id);
        }
        this.end = ts;
    };
    Span.prototype.onCPU = function (ts) {
        if (!this.isOpen()) {
            throw new Error("OnCPU for closed span " + this.id);
        }
        if (this.scheduled.length > 0) {
            var last = this.scheduled[this.scheduled.length - 1];
            if (last.isOpen()) {
                throw new Error("Double open on span " + this.id);
            }
        }
        var trace = new OnCPU(ts);
        this.scheduled.push(trace);
    };
    Span.prototype.offCPU = function (ts) {
        if (!this.isOpen()) {
            throw new Error("OffCPU for closed span " + this.id);
        }
        if (this.scheduled.length === 0) {
            throw new Error("Missing trace for " + event);
        }
        var last = this.scheduled[this.scheduled.length - 1];
        last.close(ts);
    };
    return Span;
}());
exports.Span = Span;
var Wakeup = /** @class */ (function () {
    function Wakeup(id, waking_id, parked_id, start_ts) {
        this.id = id;
        this.waking_id = waking_id;
        this.parked_id = parked_id;
        this.start_ts = start_ts;
    }
    return Wakeup;
}());
var Thread = /** @class */ (function () {
    function Thread(id) {
        this.id = id;
        this.timestamps = [];
        this.counts = [];
        this.maxCount = 0;
    }
    Thread.prototype.maxTime = function () {
        if (this.timestamps.length > 0) {
            return this.timestamps[this.timestamps.length - 1];
        }
        return 0;
    };
    return Thread;
}());
var SpanManager = /** @class */ (function () {
    function SpanManager() {
        this.spans = {};
        this.threads = {};
        this.wakeups = [];
        // Maps from a waking Span id to the wakeup. These are removed when `AsyncOnCPU` events arrive.
        this.openWakeups = {};
        this.maxTime = 0;
        this.lanes = {};
        this.laneByIndex = [];
        this.nextLaneID = 0;
    }
    SpanManager.prototype.getSpan = function (id) {
        var span = this.spans[id];
        if (!span) {
            throw new Error("Missing span ID " + id);
        }
        return span;
    };
    SpanManager.prototype.numLanes = function () {
        return this.laneByIndex.length;
    };
    SpanManager.prototype.insertLane = function (lane) {
        var at = lane.index;
        if (at > this.numLanes()) {
            throw new Error("Bad lane insertion: " + at);
        }
        if (this.lanes[lane.id]) {
            throw new Error("Duplicate lane Id: " + lane.id);
        }
        for (var laneID in this.lanes) {
            var lane_2 = this.lanes[laneID];
            if (lane_2.index >= at) {
                lane_2.index++;
            }
        }
        this.lanes[lane.id] = lane;
        var laneByIndex = [];
        for (var laneID in this.lanes) {
            var lane_3 = this.lanes[laneID];
            laneByIndex[lane_3.index] = lane_3.id;
        }
        this.laneByIndex = laneByIndex;
    };
    SpanManager.prototype.listLanes = function () {
        var _this = this;
        return this.laneByIndex.map(function (laneID) { return _this.lanes[laneID]; });
    };
    SpanManager.prototype.assignLane = function (span) {
        if (span.parent_id === null) {
            // Always push new threads at the end.
            var index = this.numLanes();
            var lane_4 = new lane_1.Lane(this.nextLaneID++, index, span);
            span.laneID = lane_4.id;
            span.maxSubtreeLaneID = lane_4.id;
            this.insertLane(lane_4);
            return lane_4;
        }
        var parent = this.getSpan(span.parent_id);
        var parentLane = this.lanes[parent.laneID];
        var curID = span.parent_id;
        while (curID !== null) {
            var ancestor = this.getSpan(curID);
            var candidates = [];
            for (var laneID in ancestor.freeLanes) {
                var lane_5 = this.lanes[laneID];
                if (lane_5.index > parentLane.index) {
                    candidates.push(lane_5.index);
                }
            }
            if (candidates.length > 0) {
                candidates.sort();
                var lane_6 = this.lanes[this.laneByIndex[candidates[0]]];
                if (!lane_6.push(span)) {
                    throw new Error("Overlap on free lane?");
                }
                delete ancestor.freeLanes[lane_6.id];
                span.laneID = lane_6.id;
                span.maxSubtreeLaneID = lane_6.id;
                return lane_6;
            }
            curID = ancestor.parent_id;
        }
        var maxLane = this.lanes[parent.maxSubtreeLaneID];
        var lane = new lane_1.Lane(this.nextLaneID++, maxLane.index + 1, span);
        span.laneID = lane.id;
        span.maxSubtreeLaneID = lane.id;
        this.insertLane(lane);
        return lane;
    };
    SpanManager.prototype.addSpan = function (span) {
        if (this.spans[span.id]) {
            throw new Error("Duplicate span ID " + span.id);
        }
        this.spans[span.id] = span;
        var assignedLane = this.assignLane(span);
        var curID = span.parent_id;
        while (curID !== null) {
            var ancestor = this.getSpan(curID);
            var lane = this.lanes[ancestor.maxSubtreeLaneID];
            if (lane.index < assignedLane.index) {
                ancestor.maxSubtreeLaneID = assignedLane.id;
            }
            curID = ancestor.parent_id;
        }
    };
    SpanManager.prototype.addSpanWithParent = function (start) {
        var parent = this.getSpan(start.parent_id);
        var span = new Span(start.name, start.id, start.parent_id, this.convertTs(start.ts), start.metadata, parent.threadName);
        this.addSpan(span);
        return span;
    };
    SpanManager.prototype.closeSpan = function (span, ts) {
        if (span.parent_id) {
            var parent_1 = this.getSpan(span.parent_id);
            parent_1.freeLanes[span.laneID] = true;
            for (var laneID in span.freeLanes) {
                parent_1.freeLanes[laneID] = true;
            }
        }
        span.close(ts);
    };
    SpanManager.prototype.addEvent = function (event) {
        if (event.AsyncStart) {
            this.addSpanWithParent(event.AsyncStart);
        }
        else if (event.AsyncOnCPU) {
            var span = this.getSpan(event.AsyncOnCPU.id);
            // Close any outstanding Wakeups.
            var wakeups = this.openWakeups[event.AsyncOnCPU.id];
            if (wakeups) {
                for (var _i = 0, wakeups_1 = wakeups; _i < wakeups_1.length; _i++) {
                    var w = wakeups_1[_i];
                    w.end_ts = this.convertTs(event.AsyncOnCPU.ts);
                }
            }
            delete this.openWakeups[event.AsyncOnCPU.id];
            var ts = this.convertTs(event.AsyncOnCPU.ts);
            span.onCPU(this.convertTs(event.AsyncOnCPU.ts));
        }
        else if (event.AsyncOffCPU) {
            var span = this.getSpan(event.AsyncOffCPU.id);
            var ts = this.convertTs(event.AsyncOffCPU.ts);
            span.offCPU(ts);
        }
        else if (event.AsyncEnd) {
            var span = this.getSpan(event.AsyncEnd.id);
            var ts = this.convertTs(event.AsyncEnd.ts);
            span.outcome = event.outcome;
            this.closeSpan(span, ts);
        }
        else if (event.SyncStart) {
            var span = this.addSpanWithParent(event.SyncStart);
            var ts = this.convertTs(event.SyncStart.ts);
            span.onCPU(ts);
        }
        else if (event.SyncEnd) {
            var span = this.getSpan(event.SyncEnd.id);
            if (span.scheduled.length !== 1) {
                throw new Error("More than one schedule for sync span " + span.id);
            }
            var ts = this.convertTs(event.SyncEnd.ts);
            span.offCPU(ts);
            this.closeSpan(span, ts);
        }
        else if (event.ThreadStart) {
            var start = event.ThreadStart;
            if (this.threads[start.name]) {
                throw new Error("Duplicate thread name " + start.name);
            }
            var span = new Span(start.name, start.id, null, // No parent on threads
            this.convertTs(start.ts), null, // No metadata on threads
            start.name);
            this.addSpan(span);
            this.threads[start.name] = new Thread(start.id);
        }
        else if (event.ThreadEnd) {
            var span = this.getSpan(event.ThreadEnd.id);
            this.closeSpan(span, this.convertTs(event.ThreadEnd.ts));
        }
        else if (event.Wakeup) {
            if (event.Wakeup.parked_span == event.Wakeup.waking_span) {
                // We don't track self-wakeups.
                return;
            }
            var wakeup = new Wakeup(this.wakeups.length, event.Wakeup.waking_span, event.Wakeup.parked_span, this.convertTs(event.Wakeup.ts));
            if (!(event.Wakeup.parked_span in this.openWakeups)) {
                this.openWakeups[event.Wakeup.parked_span] = [];
            }
            this.openWakeups[event.Wakeup.parked_span].push(wakeup);
            this.wakeups.push(wakeup);
        }
        else {
            throw new Error("Unexpected event: " + event);
        }
    };
    SpanManager.prototype.convertTs = function (ts) {
        if (typeof ts !== "number") {
            ts = ts.secs + ts.nanos * 1e-9;
        }
        if (ts > this.maxTime) {
            this.maxTime = ts;
        }
        return ts;
    };
    return SpanManager;
}());
exports.SpanManager = SpanManager;
