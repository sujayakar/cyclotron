import PIXI = require("pixi.js");

export class OnCPU {
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

    public rect;

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

        this.laneID = null;
        this.freeLanes = {};
        this.maxSubtreeLaneID = null;

        this.rect = new PIXI.Graphics();
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

    private draw(endTs) {
        this.rect.clear();
        this.rect.beginFill(0x484848);
        this.rect.drawRect(
            this.start,
            0,
            endTs - this.start,
            0.9,
        );
        this.rect.endFill();
    }

    public updateMaxTs(maxTs) {
        if (!this.isOpen()) {
            return;
        }
        this.draw(maxTs);
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
        this.draw(ts);
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
