class OnCPU {
    public start;
    public end;

    constructor(start) {
        this.start = start;
        this.end = null;
    }

    public isOpen() {
        return this.end === null;
    }

    public close(ts) {
        if (!this.isOpen()) {
            throw new Error("Double close on trace");
        }
        this.end = ts;
    }
}

class LoggedSpan {
    public name;
    public id;
    public parent_id;
    public start;
    public end;

    public scheduled;

    public metadata;
    public outcome;

    // Derived from `parent_id` pointers.
    private children;

    constructor(name, id, parent_id, start, metadata) {
        this.name = name;
        this.id = id;
        this.parent_id = parent_id;
        this.start = start;
        this.end = null;

        this.scheduled = [];

        this.metadata = metadata;
        this.outcome = null;

        this.children = [];
    }

    public isOpen() {
        return this.end === null;
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
}


class SpanManager {
    private spans;
    private threads;

    constructor() {
        this.spans = {};
        this.threads = {};
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

    private spanStart(start) {
        let parent = this.getSpan(start.parent_id);
        let span = new LoggedSpan(
            start.name,
            start.id,
            start.parent_id,
            convertTs(start.ts),
            start.metadata,
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
            this.spanStart(event.AsyncStart);
        } else if (event.AsyncOnCPU) {
            let span = this.getSpan(event.AsyncOnCPU.id);
            span.onCPU(convertTs(event.AsyncOnCPU.ts));
        } else if (event.AsyncOffCPU) {
            let span = this.getSpan(event.AsyncOffCPU.id);
            span.offCPU(convertTs(event.AsyncOffCPU.ts));
        } else if (event.AsyncEnd) {
            let span = this.getSpan(event.AsyncEnd.id);
            span.close(convertTs(event.AsyncEnd.ts));
            span.outcome = event.outcome;
        } else if (event.SyncStart) {
            let span = this.spanStart(event.SyncStart);
            span.onCPU(convertTs(event.SyncStart.ts));
        } else if (event.SyncEnd) {
            let span = this.getSpan(event.SyncEnd.id);
            if (span.scheduled.length !== 1) {
                throw new Error("More than one schedule for sync span " + span.id);
            }
            let ts = convertTs(event.SyncEnd.ts)
            span.offCPU(ts);
            span.close(ts);
        } else if (event.ThreadStart) {
            let start = event.ThreadStart;
            if (this.threads[start.name]) {
                throw new Error("Duplicate thread name " + start.name);
            }
            let span = new LoggedSpan(
                start.name,
                start.id,
                null, // No parent on threads
                convertTs(start.ts),
                null, // No metadata on threads
            );
            this.addSpan(span);
            this.threads[start.name] = start.id;
        } else if (event.ThreadEnd) {
            let span = this.getSpan(event.ThreadEnd.id);
            span.close(convertTs(event.ThreadEnd.ts));
        } else if (event.Wakeup) {
            //console.log(event);
        } else {
            throw new Error("Unexpected event: " + event);
        }
    }
}

function convertTs(ts) {
    return ts.secs + ts.nanos * 1e-9;
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
