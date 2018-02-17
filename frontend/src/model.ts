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

class Span {
    public end;
    public scheduled;
    public outcome;

    // Derived from `parent_id` pointers.
    private children;
    public expanded;

    constructor(
        readonly name: string,
        readonly id: number,
        readonly parent_id: number,
        readonly parent_ix: number,
        readonly start: number,
        readonly metadata,
        readonly threadName
    ) {
        this.end = null;
        this.scheduled = [];
        this.outcome = null;

        this.children = [];
        this.expanded = false;
    }

    public getChildren(forceExpanded) {
        if (this.expanded || forceExpanded) {
            return this.children;
        } else {
            return [];
        }
    }

    public overlappingChildren(start, end) {
        if (this.expanded) {
            return this.children.filter(span => span.intersects(start, end));
        } else {
            return [];
        }
    }

    public isOpen() {
        return this.end === null;
    }

    public mergeable(span) {
        if (this.isOpen()) {
            return false;
        }
        if (span instanceof Root) {
            return false;
        }
        return this.parent_id === span.parent_id && this.end < span.start;
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
        return `Span(id: ${this.id}, name: ${this.name}, start: ${this.start}, end: ${this.end})`;
    }
}

class Root {
    public id;
    public start;
    public end;

    constructor(public manager) {
        this.id = "root";
        this.start = 0;
        this.end = manager.maxTime;
    }

    public overlappingChildren(start, end) {
        return this.getChildren(null).filter(span => span.intersects(start, end));
    }

    public intersects(start, end) {
        return false;
    }

    public mergeable(span) {
        return false;
    }

    public isOpen() {
        return true;
    }

    public isRoot() {
        return true;
    }

    public getChildren(force) {
        return Object.keys(this.manager.threads)
            .sort()
            .map(k => this.manager.spans[this.manager.threads[k].id]);
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

    public logActivity(ts) {
        let msInt = Math.floor(ts * 1e3);

        if (this.timestamps.length > 0) {
            let last = this.timestamps[this.timestamps.length - 1];
            if (last > msInt) {
                throw new Error("Going back in time for " + this.id);
            }
            if (last === msInt) {
                let newCount = this.counts[this.counts.length - 1] + 1;
                this.counts[this.counts.length - 1] = newCount;
                if (newCount > this.maxCount) {
                    this.maxCount = newCount;
                }
                return;
            }
        }

        if (this.maxCount < 1) {
            this.maxCount = 1;
        }
        this.timestamps.push(msInt);
        this.counts.push(1);
    }

    public drawActivity(ctx) {
        if (this.timestamps.length === 0) {
            return;
        }

        ctx.fillStyle = "green";
        ctx.clearRect(0, 0, this.timestamps[this.timestamps.length - 1], 1);

        let cur_ms = -1;
        this.timestamps.forEach((ts, i) => {
            let count = this.counts[i];

            ctx.fillRect(ts, 0, 1, 1);
        });
    }
}


class SpanManager {
    public spans;
    public threads;
    public maxTime;
    public wakeups;

    private openWakeups;

    constructor() {
        this.spans = {};
        this.threads = {};
        this.wakeups = [];
        // Maps from a waking Span id to the wakeup. These are removed when `AsyncOnCPU` events arrive.
        this.openWakeups = {};
        this.maxTime = 0;
    }

    public getThread(name) {
        return this.spans[this.threads[name].id];
    }

    private getSpan(id) {
        let span = this.spans[id];
        if (!span) {
            throw new Error("Missing span ID " + id);
        }
        return span;
    }

    private addSpan(span) {
        if (this.spans[span.id]) {
            throw new Error("Duplicate span ID " + span.id);
        }
        this.spans[span.id] = span
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

    private logActivity(span, ts) {
        let thread = this.threads[span.threadName];
        thread.logActivity(ts);
    }

    private spanStart(start) {
        let parent = this.getSpan(start.parent_id);
        let span = new Span(
            start.name,
            start.id,
            start.parent_id,
            parent.children.length,
            this.convertTs(start.ts),
            start.metadata,
            parent.threadName,
        );

        if (parent.children.length > 0) {
            let last = parent.children[parent.children.length - 1];
            if (last.start > span.start) {
                throw new Error("Start times out of order for " + last.id + " and " + span.id);
            }
        }
        parent.children.push(span);
        this.addSpan(span);

        return span;
    }

    public addEvent(event) {
        if (event.AsyncStart) {
            let span = this.spanStart(event.AsyncStart);
            let ts = this.convertTs(event.AsyncStart.ts);
            this.logActivity(span, ts);
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
            this.logActivity(span, ts);
            span.onCPU(this.convertTs(event.AsyncOnCPU.ts));
        } else if (event.AsyncOffCPU) {
            let span = this.getSpan(event.AsyncOffCPU.id);
            let ts = this.convertTs(event.AsyncOffCPU.ts);
            this.logActivity(span, ts);
            span.offCPU(ts);
        } else if (event.AsyncEnd) {
            let span = this.getSpan(event.AsyncEnd.id);
            let ts = this.convertTs(event.AsyncEnd.ts);
            this.logActivity(span, ts);
            span.close(ts);
            span.outcome = event.outcome;
        } else if (event.SyncStart) {
            let span = this.spanStart(event.SyncStart);
            let ts = this.convertTs(event.SyncStart.ts);
            this.logActivity(span, ts);
            span.onCPU(ts);
        } else if (event.SyncEnd) {
            let span = this.getSpan(event.SyncEnd.id);
            if (span.scheduled.length !== 1) {
                throw new Error("More than one schedule for sync span " + span.id);
            }
            let ts = this.convertTs(event.SyncEnd.ts)
            this.logActivity(span, ts);
            span.offCPU(ts);
            span.close(ts);
        } else if (event.ThreadStart) {
            let start = event.ThreadStart;
            if (this.threads[start.name]) {
                throw new Error("Duplicate thread name " + start.name);
            }
            let span = new Span(
                start.name,
                start.id,
                null, // No parent on threads
                null,
                this.convertTs(start.ts),
                null, // No metadata on threads
                start.name,
            );
            this.addSpan(span);
            this.threads[start.name] = new Thread(start.id);
        } else if (event.ThreadEnd) {
            let span = this.getSpan(event.ThreadEnd.id);
            span.close(this.convertTs(event.ThreadEnd.ts));
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
}

function ws_test() {
    let spanManager = new SpanManager();

    var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
    socket.onmessage = function (event) {
        var msg = JSON.parse(event.data);
        var received = document.getElementById("received");
        received.appendChild(document.createElement("br"));
        received.appendChild(document.createTextNode(event.data));
        spanManager.addEvent(msg);
	};
    socket.onopen = function(event) {
        socket.send("test.log");
    };
    socket.onerror = function(event) {
        console.log("onerror", event);
    };
    socket.onclose = function(event) {
        console.log("onclose", event);
    };

    return spanManager;
}
