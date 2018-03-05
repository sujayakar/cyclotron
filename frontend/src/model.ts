import { Lane } from "./lane";
import { OnCPU, Span } from "./span";

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
    public dirty;

    constructor(private timeline) {
        this.spans = {};
        this.threads = {};
        this.wakeups = [];
        // Maps from a waking Span id to the wakeup. These are removed when `AsyncOnCPU` events arrive.
        this.openWakeups = {};
        this.maxTime = 0;

        this.lanes = {};
        this.laneByIndex = [];
        this.nextLaneID = 0;

        this.dirty = false;
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

        this.timeline.addChild(lane.container);
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
                candidates.sort((a, b) => a - b);
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

        if (span.parent_id !== null) {
            let parent = this.getSpan(span.parent_id);
            parent.children.push(span);
            span.rect.visible = parent.inheritVisible;
        }

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
            this,
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
                this,
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
        this.dirty = true;
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
