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

    public container;
    public rectangle;

    public children: Array<Span>;
    public inheritVisible: boolean;
    public text;

    constructor(
        readonly name: string,
        readonly id: number,
        readonly parent_id: number,
        readonly start: number,
        readonly metadata,
        readonly threadName,
        readonly manager
    ) {
        this.end = null;
        this.scheduled = [];
        this.outcome = null;

        this.laneID = null;
        this.freeLanes = {};
        this.maxSubtreeLaneID = null;

        this.rectangle = new PIXI.Graphics();
        this.container = new PIXI.Container();
        this.container.addChild(this.rectangle);

        this.children = [];
        this.inheritVisible = true;

        this.container.interactive = true;
        this.container.buttonMode = true;
        this.container.on("click", () => this.toggleCollapsed());

        let style = new PIXI.TextStyle({fill: "white"});
        this.text = new PIXI.Text(this.name, style);
    }

    public toggleCollapsed() {
        if (this.inheritVisible) {
            console.log(`Collapsing ${this.id}`);
            this.collapse();
        } else {
            console.log(`Expanding ${this.id}`);
            this.expand();
        }
        this.manager.dirty = true;
    }

    public collapse() {
        this.inheritVisible = false;
        let stack = this.children.slice(0);
        while (stack.length > 0) {
            let element = stack.pop();
            element.container.visible = false;
            stack.push(...element.children);
        }
    }

    public expand() {
        this.inheritVisible = true;
        let stack = this.children.slice(0);
        while (stack.length > 0) {
            let element = stack.pop();
            element.container.visible = true;

            if (element.inheritVisible) {
                stack.push(...element.children);
            }
        }
    }

    public isOpen() {
        return this.end === null;
    }

    public overlaps(start, end) {
        if (!this.container.visible) {
            return false;
        }
        return this.start < end && (this.end === null || this.end > start);
    }

    private draw(endTs) {
        this.rectangle.clear();
        this.rectangle.beginFill(0x484848);
        this.rectangle.drawRect(
            this.start,
            0,
            endTs - this.start,
            0.9,
        );
        this.rectangle.endFill();
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

        let rect = new PIXI.Graphics();
        rect.beginFill(0x00ff00, 0.15);
        rect.drawRect(
            last.start,
            0,
            ts - last.start,
            0.9,
        )
        rect.endFill();
        this.container.addChild(rect);
    }

    public toString = () : string => {
        return `Span(id: ${this.id}, parent_id: ${this.parent_id}, name: ${this.name}, start: ${this.start}, end: ${this.end})`;
    }
}
