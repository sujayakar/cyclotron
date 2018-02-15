class Cyclotron {
    private svgChart;
    private topAxis;
    private mainPanel;
    private scrubberPanel;
    private spanManager;

    private layoutMainWidth;
    private layoutMainHeight;
    private layoutScrubberHeight;
    private layoutTimelineHeight;

    private scaleX: d3.ScaleLinear<number, number>;

    private scrubberStart;
    private scrubberEnd;

    private queuedRedraw;

    constructor() {
        // First some global configuration.
        document.addEventListener(
            'mousedown',
            e => e.preventDefault(),
            false
        );

        this.spanManager = new SpanManager();

        const SPAN_HEIGHT = 80;
        const MINI_SPAN_HEIGHT = 12;

        var windowWidth = window.innerWidth - 10;
        var windowHeight = window.innerHeight - 10;

        let leftPadding = 100;
        let mainHeight = windowHeight * 0.85;
        this.layoutMainHeight = mainHeight;
        let mainWidth = windowWidth - leftPadding;
        this.layoutMainWidth = mainWidth;
        let miniHeight = windowHeight * 0.1;
        this.layoutScrubberHeight = miniHeight;
        let timeHeight = windowHeight * 0.05;
        this.queuedRedraw = false;

        //scales
        this.scaleX = d3.scaleLinear()
            .domain([0, 1])
            .range([0, mainWidth]);
        this.svgChart = d3.select("body")
            .append("svg")
            .attr("width", windowWidth)
            .attr("height", windowHeight)
            .attr("class", "chart");
        this.svgChart.append("defs")
            .append("clipPath")
            .attr("id", "clip")
            .append("rect")
            .attr("width", windowWidth)
            .attr("height", mainHeight);

        let axisHeight = 20;
        this.topAxis = this.svgChart.append("g")
            .attr("transform", "translate(" + leftPadding + "," + 0 + ")")
            .attr("width", windowWidth)
            .attr("height", mainHeight)
            .attr("class", "top-axis")
            .append("g");
        this.mainPanel = this.svgChart.append("g")
            .attr("transform", "translate(" + leftPadding + "," + axisHeight + ")")
            .attr("width", windowWidth)
            .attr("height", mainHeight)
            .attr("class", "main")
            .attr("clip-path", "url(#clip)");
        this.scrubberPanel = this.svgChart.append("g")
            .attr("transform", "translate(" + leftPadding + "," + mainHeight + ")")
            .attr("width", windowWidth)
            .attr("height", miniHeight)
            .attr("class", "mini");

        // TODO: Print that we're waiting for data or something here.
        this.setupScrubber();

        var socket = new WebSocket("ws://127.0.0.1:3001", "cyclotron-ws");
        socket.onmessage = event => { this.addEvent(JSON.parse(event.data)); };
        socket.onopen = event => { socket.send("empty_file_release.log"); };
        socket.onerror = event => { alert(`Socket error ${event}`); };
        socket.onclose = event => { alert(`Socket closed ${event}`); };

        this.scrubberStart = 0;
        this.scrubberEnd = 0;
        this.queueRedraw();

        // test_events().forEach((e, i) => { setTimeout(() => { this.addEvent(e); }, 0); });
    }

    public queueRedraw() {
        if (!this.queuedRedraw) {
            this.queuedRedraw = true;
            setTimeout(() => {
                this.queuedRedraw = false;
                this.drawMain();
            }, 16);
        }
    }

    public addEvent(event) {
        this.spanManager.addEvent(event);
        this.queueRedraw();
    }

    private nodes(expanded_only = false) {
        let root = new Root(this.spanManager);
        return d3.hierarchy(root, span => span.getChildren(!expanded_only));
    }

    private scrubberStartTs() {
        return this.scaleX.invert(this.scrubberStart);
    }

    private scrubberEndTs() {
        return this.scaleX.invert(this.scrubberEnd);
    }

    private drawMain() {
        this.scaleX.domain([0, this.spanManager.maxTime]);
        this.drawScrubber();

        // Update the axis at the top.
        let scrubberDomain = [this.scrubberStartTs(), this.scrubberEndTs()];
        let axisScale = d3.scaleLinear()
            .domain(scrubberDomain)
            .range([0, this.layoutMainWidth]);
        this.topAxis.call(d3.axisBottom(axisScale).ticks(5).tickFormat((seconds: number) => {
            let domainWidth = scrubberDomain[1] - scrubberDomain[0];
            let delta = seconds - scrubberDomain[0];

            function formatTime(n, precision) {
                if (n < 0.001) {
                    return `${(n * 1e6).toFixed(precision)}us`;
                } else if (n < 1) {
                    return `${(n * 1e3).toFixed(precision)}ms`;
                } else if (n < 60) {
                    return `${n.toFixed(precision)}s`;
                } else {
                    return `${(n / 60).toFixed(precision)}m`;
                }
            }
            return `${formatTime(scrubberDomain[0], 0)}/${formatTime(delta, 2)}`;
        }));

        let hierarchy = this.nodes(true);

        let visItems = hierarchy.descendants().filter(d => {
            return d.data.intersects(this.scrubberStartTs(), this.scrubberEndTs());
        });

        // Compute a new order based on what's visible.
        let prev = null;

        let heightMap = this.computeHeights(hierarchy, this.scrubberStartTs(), this.scrubberEndTs());
        let index = heightMap["index"];

        var x1 = d3.scaleLinear().range([0, this.layoutMainWidth]);
        x1.domain([this.scrubberStartTs(), this.scrubberEndTs()]);

        // This scales all the spans to share the vertical space when they're fully expanded.
        //
        // We might want to use a fixed height here and scroll instead.
        let viewHeight = this.layoutMainHeight - 60;
        let defaultHeight = 40 * index;
        if (defaultHeight < viewHeight) {
            viewHeight = defaultHeight;
        }
        var yScale = d3.scaleLinear()
            .domain([0, index])
            .range([0, viewHeight]);

        let clickHandler = node => { // we should set this up once at the beginning
            console.log("got clicked: " + node.data.name);
            node.data.expanded = !node.data.expanded;
            this.queueRedraw();
        };

        let xPosition = d => { return x1(d.data.start); };
        let yPosition = d => { return yScale(heightMap[d.data.id]); };
        let computeWidth = d => { return x1(this.spanEnd(d.data)) - x1(d.data.start); };
        let computeHeight = d => { return .95 * yScale(1); };

        // For already-visible spans, make sure they're positioned appropriately.
        //
        // Note that we animate changes on the y-axis, but not on the x-axis. This is so
        // that when you scroll side-to-side, things update immediately.
        let rects = this.mainPanel.selectAll("rect") // formerly itemRects
            .data(visItems, (d: any) => { return d.data.id; })
            .attr("x", xPosition)
            .attr("width", computeWidth)
            .attr("height", computeHeight)
            .on("click", clickHandler)
            .style("opacity", 1.0);

        // If things shift vertically, we animate them to their new positions.
        //
        // Note that this cancels the previous transition from when the object was newly created,
        // so it should match exactly.
        rects.transition()
            .duration(100)
            .ease(d3.easeLinear)
            .style("opacity", 1.0)
            .attr("y", yPosition);

        // For new entries, do the things.
        let color = d3.scaleLinear<string>()
            .domain([0, 0.01, this.spanManager.maxTime])
            .clamp(true)
            .range(["#4caf50", "#e88b01", "#af4c4c"]);
        let newRects = rects.enter().append("rect")
            .attr("class", d => { return "span"; })
            .attr("x", xPosition)
            .attr("y", yPosition)
            .attr("width", computeWidth)
            .attr("height", computeHeight)
            .on("click", clickHandler)
            .style("opacity", 0.7)
            .style("fill", d => { return color(this.spanEnd(d.data) - d.data.start);});

        newRects.transition()
            .duration(150)
            .style("opacity", 1.0)
            .attr("y", yPosition);

        rects.exit().remove();

        // same deal w/ the text
        var labels = this.mainPanel.selectAll("text") // formerly itemRects.selectAll("text")
            .data(visItems, (d: any) => { return d.data.id; })
            .attr("x", d => { return x1(Math.max(d.data.start, this.scrubberStartTs())); })
            .attr("y", d => { return yScale(heightMap[d.data.id]) + 20; });

        labels.enter().append("text")
            .text(d => { return d.data.name; })
            .attr("class", "span-text") // why doesn't this work? why inline fill???
            .attr("x", xPosition)
            .attr("y", d => { return yScale(heightMap[d.data.id]) + 20; })
            .attr("text-anchor", "start");

        labels.exit().remove();
    }

    private setupScrubber() {
        var brush = d3.brushX()
            .extent([[0, 0], [this.layoutMainWidth, this.layoutScrubberHeight]])
            .on("brush", () => {
                this.scrubberStart = d3.event.selection[0];
                this.scrubberEnd = d3.event.selection[1];
                this.drawMain();
            });
        this.scrubberPanel.append("g")
            .attr("class", "x brush")
            .call(brush)
            .selectAll("rect")
            .attr("y", 0)
            .attr("height", this.layoutScrubberHeight);
    }

    private spanEnd(span) {
        return span.end || this.spanManager.maxTime;
    }

    private computeHeights(hierarchy, startTs, endTs) {
        var heightMap = {};
        var index = -1;
        var node = hierarchy, stack = [hierarchy], children, i;
        var prev = null;

        while (node = stack.pop()) {
            let span = node.data;
            if (span.intersects(startTs, endTs)) {
                if (prev && prev.mergeable(span)) {
                    heightMap[span.id] = index;
                } else {
                    heightMap[span.id] = ++index;
                }
                prev = span;
            }

            children = node.children;
            if (children) for (i = children.length - 1; i >= 0; --i) {
                stack.push(children[i])
            }
        }

        if (index < 0) {
            index = 0;
        }

        heightMap["index"] = index;
        return heightMap;
    }

    // private drawWakeups() {
    //     // Okay, now draw the arrows
    //     let toDraw = this.spanManager.wakeups.filter(w => {
    //         if (!map[w.waking_id] || !map[w.parked_id]) {
    //             return false;
    //         }
    //         if (w.ts < scrubberDomain[0] || w.ts > scrubberDomain[1]) {
    //             return false;
    //         }
    //         return true;
    //     });
    //     console.log(`Found ${toDraw.length} arrows`);
    //     let wakeups = this.mainPanel.selectAll("line")
    //         .data(toDraw, (d) => { return d.id })
    //         .attr("x1", (d) => { return x1(d.ts) })
    //         .attr("y1", (d) => { return yScale(map[d.waking_id]) })
    //         .attr("x2", (d) => { return x1(d.ts) + 5 })
    //         .attr("y2", (d) => { return yScale(map[d.waking_id]) + 5 })
    //         .style("stroke", "rgb(255,0,0)")
    //         .style("stroke-width", "2");

    //     wakeups.enter().append("line")
    //         .data(toDraw, (d) => { return d.id })
    //         .attr("x1", (d) => { return x1(d.ts) })
    //         .attr("y1", (d) => { return yScale(map[d.waking_id]) })
    //         .attr("x2", (d) => { return x1(d.ts) + 5 })
    //         .attr("y2", (d) => { return yScale(map[d.parked_id]) })
    //         .style("stroke", "rgb(255,0,0)")
    //         .style("stroke-width", "1");

    //     wakeups.exit().remove();
    // }

    private drawScrubber() {
        this.scaleX.domain([0, this.spanManager.maxTime]);

        // Compute the layout.
        let map = {};
        let index = -1;
        let hierarchy = this.nodes();
        hierarchy.eachBefore(n => {
            map[n.data.id] = {
                rowIdx: ++index
            }
        })

        let count: number = hierarchy.descendants().length;

        // In the scrubber we always show everything expanded (for now).
        var yScaleMini = d3.scaleLinear()
            .domain([0, count])
            .range([0, this.layoutScrubberHeight]);

        let minis = this.scrubberPanel.selectAll(".miniItems")
            .data(hierarchy.descendants(), d => { return d.data.id; })
            .attr("x", d => { return this.scaleX(d.data.start); })
            .attr("y", d => { return yScaleMini(map[d.data.id].rowIdx) - 5; })
            .attr("width", d => this.scaleX(this.spanEnd(d.data) - d.data.start));

        minis.enter().append("rect")
            .attr("class", d => { return "miniItems" })
            .attr("x", d => { return this.scaleX(d.data.start); })
            .attr("y", n => { return yScaleMini(map[n.data.id].rowIdx) - 5; })
            .attr("width", d => this.scaleX(this.spanEnd(d.data) - d.data.start))
            .attr("height", 1);

        minis.exit().remove();
    }
}

new Cyclotron();

function test_events() {
    return [
        {ThreadStart: {name: "Control", id: 0, ts: 0}},
        {AsyncStart: {name: "Scheduler", parent_id: 0, id: 1, ts: 0.10}},
        {AsyncStart: {name: "Downloader", parent_id: 0, id: 2, ts: 0.20}},
        {AsyncStart: {name: "PreLocal", parent_id: 0, id: 3, ts: 265}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 4, ts: 300}},
        {AsyncEnd: {id: 3, ts: 420, outcome: "Success"}},
        {AsyncEnd: {id: 4, ts: 530, outcome: "Success"}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 5, ts: 550}},
        {AsyncStart: {name: "RemoteAdd(/foo)", parent_id: 1, id: 6, ts: 580}},
        {AsyncEnd: {id: 6, ts: 615, outcome: "Success"}},
        {AsyncStart: {name: "RemoteAdd(/bar)", parent_id: 1, id: 7, ts: 620}},
        {AsyncEnd: {id: 5, ts: 700, outcome: "Success"}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 8, ts: 710}},
        {AsyncEnd: {id: 8, ts: 790, outcome: "Success"}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 9, ts: 800}},
        {AsyncEnd: {id: 7, ts: 900, outcome: "Success"}},
        {AsyncStart: {name: "RemoteAdd(/baz)", parent_id: 1, id: 10, ts: 960}},
        {AsyncEnd: {id: 9, ts: 1180, outcome: "Success"}},
        {AsyncEnd: {id: 10, ts: 1265, outcome: "Success"}},
        {AsyncStart: {name: "RemoteAdd(/bang)", parent_id: 1, id: 11, ts: 1270}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 12, ts: 1270}},
        {AsyncEnd: {id: 11, ts: 1360, outcome: "Success"}},
        {AsyncEnd: {id: 12, ts: 1365, outcome: "Success"}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 13, ts: 1365}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 14, ts: 1370}},
        {AsyncEnd: {id: 14, ts: 1700, outcome: "Success"}},
        {AsyncEnd: {id: 13, ts: 1800, outcome: "Success"}},
        {AsyncEnd: {id: 1, ts: 2000, outcome: "Success"}},
        {AsyncEnd: {id: 2, ts: 2000, outcome: "Success"}},
        {ThreadEnd: {id: 0, ts: 2000}},
    ];
}
