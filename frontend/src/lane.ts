import { Span, SpanManager } from "./model";

export class Lane {
    // Sorted by increasing start time, non-overlapping
    public spans: Array<Span>;

    constructor(public id: number, public index: number, span: Span) {
        this.spans = [span];
    }

    public isOpen(): boolean {
        return this.lastSpan().isOpen();
    }

    public lastSpan(): Span {
        return this.spans[this.spans.length - 1];
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
        return true;
    }
}
