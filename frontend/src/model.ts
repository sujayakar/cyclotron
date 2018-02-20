import { Lane } from "./lane";

class OnCPU {
    public end;

    constructor(readonly start: number) {
        this.end = null;
    }

    public isOpen() {
        return this.end === null;
    }

    public close(ts) {
        if (!this.isOpen()) {
            throw new Error("Double close on trace");
        }
        if (this.start > ts) {
            throw new Error("Trace wth start after end");
        }
        this.end = ts;
    }
}

export class Span {
    public end;
    public scheduled;
    public outcome;

    public laneID;
    public freeLanes;
    public maxSubtreeLaneID;

    // Derived from `parent_id` pointers.
    public expanded;

    constructor(
        readonly name: string,
        readonly id: number,
        readonly parent_id: number,
        readonly start: number,
        readonly metadata,
        readonly threadName
    ) {
        this.end = null;
        this.scheduled = [];
        this.outcome = null;

        this.expanded = true;
        this.laneID = null;
        this.freeLanes = {};
        this.maxSubtreeLaneID = null;
    }

    public isOpen() {
        return this.end === null;
    }

    public intersects(start, end) {
        return this.start < end && (this.end === null || this.end > start);
    }

    public overlaps(span) {
        let first  = this.start < span.start ? this : span;
        let second = this.start < span.start ? span : this;
        return first.end === null || second.start < first.end;
    }

    public close(ts) {
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
    }

    public onCPU(ts) {
        if (!this.isOpen()) {
            throw new Error("OnCPU for closed span " + this.id);
        }
        if (this.scheduled.length > 0) {
            let last = this.scheduled[this.scheduled.length - 1];
            if (last.isOpen()) {
                throw new Error("Double open on span " + this.id);
            }
        }
        let trace = new OnCPU(ts);
        this.scheduled.push(trace);
    }

    public offCPU(ts) {
        if (!this.isOpen()) {
            throw new Error("OffCPU for closed span " + this.id);
        }
        if (this.scheduled.length === 0) {
            throw new Error("Missing trace for " + event);
        }
        let last = this.scheduled[this.scheduled.length - 1];
        last.close(ts);
    }

    public toString = () : string => {
        return `Span(id: ${this.id}, parent_id: ${this.parent_id}, name: ${this.name}, start: ${this.start}, end: ${this.end})`;
    }
}

class Wakeup {
    public end_ts;
    constructor(public id, public waking_id, public parked_id, public start_ts) {}
}


class Thread {
    public counts;
    public timestamps;
    public maxCount;

    constructor(public id) {
        this.timestamps = [];
        this.counts = [];
        this.maxCount = 0;
    }

    public maxTime() {
        if (this.timestamps.length > 0) {
            return this.timestamps[this.timestamps.length - 1];
        }
        return 0;
    }
}

export class SpanManager {
    public spans;
    public threads;
    public maxTime;
    public wakeups;

    private openWakeups;

    private lanes;
    private laneByIndex;
    private nextLaneID;

    constructor() {
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

    private getSpan(id) {
        let span = this.spans[id];
        if (!span) {
            throw new Error("Missing span ID " + id);
        }
        return span;
    }

    private numLanes(): number {
        return this.laneByIndex.length;
    }

    private insertLane(lane: Lane) {
        let at = lane.index;
        if (at > this.numLanes()) {
            throw new Error(`Bad lane insertion: ${at}`);
        }
        if (this.lanes[lane.id]) {
            throw new Error(`Duplicate lane Id: ${lane.id}`);
        }
        for (let laneID in this.lanes) {
            let lane = this.lanes[laneID];
            if (lane.index >= at) {
                lane.index++;
            }
        }
        this.lanes[lane.id] = lane;

        let laneByIndex = [];
        for (let laneID in this.lanes) {
            let lane = this.lanes[laneID];
            laneByIndex[lane.index] = lane.id;
        }
        this.laneByIndex = laneByIndex;
    }

    public listLanes() {
        return this.laneByIndex.map(laneID => this.lanes[laneID]);
    }

    private assignLane(span) {
        if (span.parent_id === null) {
            // Always push new threads at the end.
            let index = this.numLanes();
            let lane = new Lane(this.nextLaneID++, index, span);
            span.laneID = lane.id;
            span.maxSubtreeLaneID = lane.id;
            this.insertLane(lane);
            console.log(`Thread ${span.name} => ${lane.index}`);
            return lane;
        }

        let parent = this.getSpan(span.parent_id);
        let parentLane = this.lanes[parent.laneID];

        let curID = span.parent_id;
        while (curID !== null) {
            let ancestor = this.getSpan(curID);

            let candidates = [];
            for (let laneID in ancestor.freeLanes) {
                let lane = this.lanes[laneID];
                if (lane.index > parentLane.index) {
                    candidates.push(lane.index);
                }
            }

            if (candidates.length > 0) {
                candidates.sort();
                console.log(`Match for ${span.name} => ${candidates[0]}`);
                let lane = this.lanes[this.laneByIndex[candidates[0]]];
                if (!lane.push(span)) {
                    throw new Error(`Overlap on free lane?`);
                }
                delete ancestor.freeLanes[lane.id];
                span.laneID = lane.id;
                span.maxSubtreeLaneID = lane.id;
                return lane;
            }

            curID = ancestor.parent_id;
        }

        let maxLane = this.lanes[parent.maxSubtreeLaneID];
        console.log(`Inserting at ${maxLane.index + 1}`);
        let lane = new Lane(this.nextLaneID++, maxLane.index + 1, span);
        span.laneID = lane.id;
        span.maxSubtreeLaneID = lane.id;
        this.insertLane(lane);
        return lane;
    }

    private addSpan(span) {
        if (this.spans[span.id]) {
            throw new Error("Duplicate span ID " + span.id);
        }
        this.spans[span.id] = span;
        let assignedLane = this.assignLane(span);

        let curID = span.parent_id;
        while (curID !== null) {
            let ancestor = this.getSpan(curID);
            let lane = this.lanes[ancestor.maxSubtreeLaneID];

            if (lane.index < assignedLane.index) {
                ancestor.maxSubtreeLaneID = assignedLane.id;
            }

            curID = ancestor.parent_id;
        }
    }

    private addSpanWithParent(start) {
        let parent = this.getSpan(start.parent_id);
        let span = new Span(
            start.name,
            start.id,
            start.parent_id,
            this.convertTs(start.ts),
            start.metadata,
            parent.threadName,
        );
        this.addSpan(span);
        return span;
    }

    private closeSpan(span, ts) {
        if (span.parent_id) {
            let parent = this.getSpan(span.parent_id);
            parent.freeLanes[span.laneID] = true;
            for (let laneID in span.freeLanes) {
                parent.freeLanes[laneID] = true;
            }
        }
        span.close(ts);
    }

    public addEvent(event) {
        if (event.AsyncStart) {
            this.addSpanWithParent(event.AsyncStart);
        } else if (event.AsyncOnCPU) {
            let span = this.getSpan(event.AsyncOnCPU.id);
            // Close any outstanding Wakeups.
            let wakeups = this.openWakeups[event.AsyncOnCPU.id];
            if (wakeups) {
                for (let w of wakeups) {
                    w.end_ts = this.convertTs(event.AsyncOnCPU.ts)
                }
            }
            delete this.openWakeups[event.AsyncOnCPU.id];
            let ts = this.convertTs(event.AsyncOnCPU.ts);
            span.onCPU(this.convertTs(event.AsyncOnCPU.ts));
        } else if (event.AsyncOffCPU) {
            let span = this.getSpan(event.AsyncOffCPU.id);
            let ts = this.convertTs(event.AsyncOffCPU.ts);
            span.offCPU(ts);
        } else if (event.AsyncEnd) {
            let span = this.getSpan(event.AsyncEnd.id);
            let ts = this.convertTs(event.AsyncEnd.ts);
            span.outcome = event.outcome;
            this.closeSpan(span, ts);
        } else if (event.SyncStart) {
            let span = this.addSpanWithParent(event.SyncStart);
            let ts = this.convertTs(event.SyncStart.ts);
            span.onCPU(ts);
        } else if (event.SyncEnd) {
            let span = this.getSpan(event.SyncEnd.id);
            if (span.scheduled.length !== 1) {
                throw new Error("More than one schedule for sync span " + span.id);
            }
            let ts = this.convertTs(event.SyncEnd.ts)
            span.offCPU(ts);
            this.closeSpan(span, ts);
        } else if (event.ThreadStart) {
            let start = event.ThreadStart;
            if (this.threads[start.name]) {
                throw new Error("Duplicate thread name " + start.name);
            }
            let span = new Span(
                start.name,
                start.id,
                null, // No parent on threads
                this.convertTs(start.ts),
                null, // No metadata on threads
                start.name,
            );
            this.addSpan(span);
            this.threads[start.name] = new Thread(start.id);
        } else if (event.ThreadEnd) {
            let span = this.getSpan(event.ThreadEnd.id);
            this.closeSpan(span, this.convertTs(event.ThreadEnd.ts));
        } else if (event.Wakeup) {
            if (event.Wakeup.parked_span == event.Wakeup.waking_span) {
                // We don't track self-wakeups.
                return;
            }
            let wakeup = new Wakeup(
                this.wakeups.length,
                event.Wakeup.waking_span,
                event.Wakeup.parked_span,
                this.convertTs(event.Wakeup.ts),
            );
            if (!(event.Wakeup.parked_span in this.openWakeups)) {
                this.openWakeups[event.Wakeup.parked_span] = [];
            }
            this.openWakeups[event.Wakeup.parked_span].push(wakeup);
            this.wakeups.push(wakeup);
        } else {
            throw new Error("Unexpected event: " + event);
        }
    }

    private convertTs(ts) {
        if (typeof ts !== "number") {
            ts = ts.secs + ts.nanos * 1e-9;
        }
        if (ts > this.maxTime) {
            this.maxTime = ts;
        }
        return ts;
    }
}
