import { Span } from "./span";
import PIXI = require("pixi.js");

export class Lane {
    // Sorted by increasing start time, non-overlapping
    public spans: Array<Span>;
    public container: PIXI.Container;

    constructor(public id: number, public index: number, span: Span) {
        this.spans = [span];
        this.container = new PIXI.Container;
        this.container.x = 0;
        this.container.y = 0;

        this.container.addChild(span.rect);
    }

    public isOpen(): boolean {
        return this.lastSpan().isOpen();
    }

    public lastSpan(): Span {
        return this.spans[this.spans.length - 1];
    }

    public overlaps(startTs, endTs): boolean {
        for (let span of this.spans) {
            if (span.intersects(startTs, endTs)) {
                return true;
            }
        }
        return false;
    }

    public push(span: Span): boolean {
        let last = this.lastSpan();
        if (last.isOpen()) {
            return false;
        }
        if (last.end > span.start) {
            return false;
        }
        if (last.start > span.start) {
            throw new Error(`Out of order span ${span}`);
        }
        this.spans.push(span);
        this.container.addChild(span.rect);
        return true;
    }

    public updateMaxTs(maxTs) {
        this.spans[this.spans.length - 1].updateMaxTs(maxTs);
    }
}
