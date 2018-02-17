class Cyclotron {
    private svgChart;
    private topAxis;
    private mainPanel;
    private scrubberPanel;
    private spanManager;
    private scrubberBrush;

    private layoutMainWidth;
    private layoutMainHeight;
    private layoutScrubberHeight;
    private layoutTimelineHeight;

    private scaleX: d3.ScaleLinear<number, number>;

    private scrubberStart;
    private scrubberEnd;

    private queuedRedraw;
    private queuedRedrawScrubber;

    constructor() {
        // First some global configuration.
        document.addEventListener(
            'mousedown',
            e => e.preventDefault(),
            false
        );

        let zoomIn = () => {
            let zoomWidth = this.scrubberEnd - this.scrubberStart;
            this.scrubberStart = d3.max([this.scrubberStart + zoomWidth * 0.05, 0]);
            this.scrubberEnd = d3.min([this.scrubberEnd - zoomWidth * 0.05, this.layoutMainWidth]);
            this.queueRedraw();
        };
        let zoomOut = () => {
            let zoomWidth = this.scrubberEnd - this.scrubberStart;
            this.scrubberStart = d3.max([this.scrubberStart - zoomWidth * 0.05, 0]);
            this.scrubberEnd = d3.min([this.scrubberEnd + zoomWidth * 0.05, this.layoutMainWidth]);
            this.queueRedraw();
        };
        let panLeft = () => {
            let zoomWidth = this.scrubberEnd - this.scrubberStart;
            this.scrubberStart = d3.max([this.scrubberStart - zoomWidth * 0.05, 0])
            this.scrubberEnd = d3.min([this.scrubberEnd - zoomWidth * 0.05, this.layoutMainWidth]);
            this.queueRedraw();
        };
        let panRight = () => {
            let zoomWidth = this.scrubberEnd - this.scrubberStart;
            this.scrubberStart = d3.max([this.scrubberStart + zoomWidth * 0.05, 0]);
            this.scrubberEnd = d3.min([this.scrubberEnd + zoomWidth * 0.05, this.layoutMainWidth]);
            this.queueRedraw();
        }

        d3.select("body")
            .on("keydown", () => {
                if (this.scrubberStart === undefined) return;
                if (d3.event.keyCode == 87) { // W
                    zoomIn();
                } else if (d3.event.keyCode == 83) { // S
                    zoomOut();
                } else if (d3.event.keyCode == 65) { // A
                    panLeft();
                } else if (d3.event.keyCode == 68) { // D
                    panRight();
                }
            })
            .on("wheel.zoom", () => {
                if (this.scrubberStart === undefined) return;
                if (d3.event.wheelDeltaY > 0) {
                    zoomIn();
                } else if (d3.event.wheelDeltaY < 0) {
                    zoomOut();
                }
            });

        this.spanManager = new SpanManager();

        var windowWidth = document.body.clientWidth;
        var windowHeight = document.documentElement.clientHeight - 20; // Some buffer to ensure no scroll.

        let mainHeight = windowHeight * 0.90;
        this.layoutMainHeight = mainHeight;
        let mainWidth = windowWidth;
        this.layoutMainWidth = mainWidth;
        let miniHeight = windowHeight * 0.05;
        this.layoutScrubberHeight = miniHeight;
        let timeHeight = windowHeight * 0.05;
        this.queuedRedraw = false;
        this.queuedRedrawScrubber = false;

        //scales
        this.scaleX = d3.scaleLinear()
            .domain([0, 1])
            .range([0, mainWidth]);
        this.svgChart = d3.select("body")
            .append("svg")
            .attr("width", windowWidth)
            .attr("height", windowHeight)
            .attr("class", "chart");
        const defs = this.svgChart.append("defs");
        defs.append("clipPath")
            .attr("id", "clip")
            .append("rect")
            .attr("width", windowWidth)
            .attr("height", mainHeight);
        defs.append("pattern")
            .attr("id", "pinstripe")
            .attr("patternUnits", "userSpaceOnUse")
            .attr("width", 60)
            .attr("height", 1)
            .attr("patternTransform", "rotate(90)")
            .append("line")
            .attr("x1", 0)
            .attr("y1", 0)
            .attr("x2", 30)
            .attr("y2", 0)
            .attr("stroke", "lightgrey")
            .attr("stroke-width", 3);

        let marker = this.svgChart.append("marker")
            .attr("id", "triangle")
            .attr("viewBox", "0 0 10 10")
            .attr("refX", 0)
            .attr("refY", 5)
            .attr("markerUnits", "strokeWidth")
            .attr("markerWidth", 4)
            .attr("markerHeight", 3)
            .attr("orient", "auto");
        marker.append("path")
            .attr("d", "M 0 0 L 10 5 L 0 10 z");

        let axisHeight = 20;

        // Add in the stripe background.
        this.svgChart.append("rect")
            .attr("id", "pinstripe-rect")
            .attr("transform", "translate(0," + axisHeight + ")")
            .attr("width", "100%")
            .attr("height", this.layoutMainHeight - axisHeight)
            .attr("fill", "url(#pinstripe)");

        // Create the scrubber on the main panel, too.
        //
        // This comes before the other rects which means it appears "behind" when rendering,
        // so we should probably bump it to the top once it starts?
        let mainScrubber = d3.brushX()
            .extent([[0, 0], [this.layoutMainWidth, this.layoutMainHeight - axisHeight]])
            .on("end", () => {
                if (!d3.event.selection) {
                    // This is fired after we clear below (i.e. recursively), so we should just return.
                    return;
                }

                if (d3.event.selection[1] - d3.event.selection[0] < 5) {
                    console.log("skipping");
                    // Hide the scrubber.
                    mainScrubber.move(d3.select("#main-brush"), null);
                    return;
                }

                // Scale based on the current viewport.
                let scale = d3.scaleLinear()
                    .domain([0, this.layoutMainWidth])
                    .range([this.scrubberStart, this.scrubberEnd]);
                this.scrubberStart = scale(d3.event.selection[0]);
                this.scrubberEnd = scale(d3.event.selection[1]);
                this.queueRedraw();

                // Hide the scrubber.
                mainScrubber.move(d3.select("#main-brush"), null);
            });
        this.svgChart.append("g")
            .attr("id", "main-brush")
            .attr("class", "brush")
            .call(mainScrubber);


        this.topAxis = this.svgChart.append("g")
            .attr("width", windowWidth)
            .attr("class", "top-axis")
            .append("g");
        this.mainPanel = this.svgChart.append("g")
            .attr("transform", "translate(0," + axisHeight + ")")
            .attr("width", windowWidth)
            .attr("height", mainHeight)
            .attr("class", "main")
            .attr("clip-path", "url(#clip)");

        this.scrubberPanel = this.svgChart.append("g")
            .attr("transform", "translate(0," + mainHeight + ")")
            .attr("width", windowWidth)
            .attr("height", miniHeight)
            .attr("class", "mini");

        this.scrubberBrush = d3.brushX()
            .extent([[0, 0], [this.layoutMainWidth, this.layoutScrubberHeight]])
            .on("brush", () => {
                console.log("BRUSHED");
                this.scrubberStart = d3.event.selection[0];
                this.scrubberEnd = d3.event.selection[1];
                this.queueRedraw(false);
            });
        this.scrubberPanel.append("g")
            .attr("id", "bottom-scrubber")
            .attr("class", "brush")
            .call(this.scrubberBrush);

        // TODO: Print that we're waiting for data or something here.
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

    public queueRedraw(redrawScrubber = true) {
        if (!this.queuedRedraw) {
            setTimeout(() => {
                let redrawScrubber = this.queuedRedrawScrubber;
                this.queuedRedraw = false;
                this.queuedRedrawScrubber = false;
                this.drawMain(redrawScrubber);
            }, 16);
            this.queuedRedraw = true;
        }
        this.queuedRedrawScrubber = redrawScrubber || this.queuedRedrawScrubber;
    }

    public addEvent(event) {
        this.spanManager.addEvent(event);
        this.queueRedraw();
    }

    private scrubberStartTs() {
        return this.scaleX.invert(this.scrubberStart);
    }

    private scrubberEndTs() {
        return this.scaleX.invert(this.scrubberEnd);
    }

    private drawMain(redrawScrubber) {
        this.scaleX.domain([0, this.spanManager.maxTime]);
        if (redrawScrubber)
            this.drawScrubber();

        // Update the axis at the top.
        let startTs = this.scrubberStartTs();
        let endTs = this.scrubberEndTs();
        let scrubberDomain = [startTs, endTs];
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

        var screenX = d3.scaleLinear().range([0, this.layoutMainWidth]);
        screenX.domain([startTs, endTs]);

        let {laneAssignment, visibleSpans, numLanes} = this.computeVisible(startTs, endTs);

        // This scales all the spans to share the vertical space when they're fully expanded.
        //
        // We might want to use a fixed height here and scroll instead.
        let viewHeight = this.layoutMainHeight - 60;
        let defaultHeight = 30 * numLanes;
        if (defaultHeight < viewHeight) {
            viewHeight = defaultHeight;
        }
        var yScale = d3.scaleLinear()
            .domain([0, numLanes])
            .range([0, viewHeight]);

        // Resize the stripes.
        d3.select("#pinstripe")
            .attr("width", yScale(2));
        d3.select("#pinstripe-rect")
            .transition()
            .duration(100)
            .attr("height", yScale(1) * (numLanes + 1));
        d3.select("#pinstripe")
            .select("line")
            .attr("x2", yScale(1));


        let clickHandler = node => { // we should set this up once at the beginning
            console.log("got clicked: " + node.name);
            node.expanded = !node.expanded;
            this.queueRedraw();
        };

        let xPosition = d => {
            let start = screenX(d.start);
            if (start < 0) {
                start = 0;
            }
            return start;
        };
        let yPosition = d => { return yScale(laneAssignment[d.id]) + 0.10 * yScale(1) };
        let computeWidth = d => {
            let start = screenX(d.start);
            if (start < 0) {
                start = 0;
            }
            let end = screenX(this.spanEnd(d));
            if (end > this.layoutMainWidth) {
                end = this.layoutMainWidth;
            }
            return end - start;
        };
        let computeHeight = d => { return .80 * yScale(1); };

        // For already-visible spans, make sure they're positioned appropriately.
        //
        // Note that we animate changes on the y-axis, but not on the x-axis. This is so
        // that when you scroll side-to-side, things update immediately.
        let svgs = this.mainPanel.selectAll("svg") // formerly itemRects
            .data(visibleSpans, (d: any) => { return d.id; })
            .attr("x", xPosition)
            .attr("width", computeWidth)
            .attr("height", computeHeight);

        // If things shift vertically, we animate them to their new positions.
        //
        // Note that this cancels the previous transition from when the object was newly created,
        // so it should match exactly.
        svgs.transition()
            .duration(100)
            .ease(d3.easeLinear)
            .attr("y", yPosition);

        let rects = svgs.select("rect")
            .attr("width", computeWidth)
            .attr("height", computeHeight)
            .on("click", clickHandler)
            .style("opacity", 1.0);

        // For new entries, do the things.
        let color = d3.scaleLinear<string>()
            .domain([0, 0.01, this.spanManager.maxTime])
            .clamp(true)
            .range(["#4caf50", "#e88b01", "#af4c4c"]);

        // Make sure text shows correctly.
        let text = d =>  {
            if (!d.children.length)
                return d.name;
            else if (d.expanded)
                return "▼ " + d.name;
            else
                return "▶ " + d.name;
        };
        svgs.selectAll("text")
            .data(visibleSpans, (d: any) => { return d.id; })
            .style("font-size", d => computeHeight(d) * 0.8)
            .attr("y", d => computeHeight(d) * 0.8)
            .text(text);

        let newSVGs = svgs.enter().append("svg")
            .attr("class", d => { return "span"; })
            .attr("x", xPosition)
            .attr("y", yPosition)
            .attr("width", computeWidth)
            .attr("height", computeHeight);

        newSVGs.transition()
            .duration(150)
            .attr("y", yPosition);

        let newRects = newSVGs.append("rect")
            .attr("width", computeWidth)
            .attr("height", computeHeight)
            .attr("rx", 6)
            .attr("ry", 6)
            .on("click", clickHandler)
            .style("opacity", 0.7)
            .style("fill", d => { return color(this.spanEnd(d) - d.start);})

        let newText = newSVGs.append("text")
            .text(text)
            .attr("x", 5)
            .style("font-size", d => computeHeight(d) * 0.8)
            .attr("y", d => computeHeight(d) * 0.8)
            .attr("class", "span-text")
            .attr("text-anchor", "start");

        newRects.transition()
            .duration(150)
            .style("opacity", 1.0);

        svgs.exit().remove();

        // Okay, now draw the arrows
        let toDraw = this.spanManager.wakeups.filter(w => {
            if (!w.end_ts) {
                return false;
            }
            if (!laneAssignment[w.waking_id] || !laneAssignment[w.parked_id]) {
                return false;
            }
            if (w.start_ts < scrubberDomain[0] || w.end_ts > scrubberDomain[1]) {
                return false;
            }
            return true;
        });

        let computeLine = (d) => {
            let x1 = screenX(d.start_ts);
            let y1 = yScale(laneAssignment[d.waking_id]) + computeHeight(d) / 2.0;
            let x2 = screenX(d.end_ts);
            let y2 = yScale(laneAssignment[d.parked_id]) + + computeHeight(d) / 2.0;

            // Compute the Bezier control points
            let cX1 = x1;
            let cY1 = 0.8 * y1 + 0.2 * y2;
            let cX2 = x1;
            let cY2 = y2

            return `M ${x1} ${y1} C ${cX1} ${cY1}, ${cX2} ${cY2}, ${x2} ${y2}`;
        };

        let wakeups = this.mainPanel.selectAll("path")
            .data(toDraw, (d) => { return d.id })
            .attr("class", "wakeup-line")
            .attr("fill-opacity", "0")
            .attr("d", computeLine)
            .attr("marker-end", "url(#triangle)");

        wakeups.enter().append("path")
            .data(toDraw, (d) => { return d.id })
            .attr("class", "wakeup-line")
            .attr("fill-opacity", "0")
            .attr("d", computeLine)
            .attr("marker-end", "url(#triangle)");

        wakeups.exit().remove();

    }

    private spanEnd(span) {
        return span.end || this.spanManager.maxTime;
    }

    private computeVisible(startTs, endTs) {
        let root = new Root(this.spanManager);
        let stack = root.overlappingChildren(startTs, endTs).reverse();

        var nextLane = 0;

        // lane -> max ts drawn
        var laneMaxTs = {}
        var laneAssignment = {};
        var visible = []

        var span;
        while (span = stack.pop()) {
            var lane = null;
            if (span.parent_id) {
                let parentLane = laneAssignment[span.parent_id];
                for (var candidate = parentLane + 1; candidate < nextLane; candidate++) {
                    let maxTs = laneMaxTs[candidate];
                    if (maxTs === undefined) {
                        throw new Error(`Missing max ts for ${lane}`);
                    }
                    if (maxTs <= span.start) {
                        lane = candidate;
                        break;
                    }
                }
            }
            if (lane === null) {
                lane = nextLane++;
            }

            laneMaxTs[lane] = span.end ? span.end : endTs;
            laneAssignment[span.id] = lane;
            visible.push(span);

            span.overlappingChildren(startTs, endTs).reverse().forEach(child => { stack.push(child); })
        }

        return {laneAssignment, visibleSpans: visible, numLanes: nextLane};
    }

    private drawScrubber() {
        // Make sure the scrubber's position is reflected.
        if (this.scrubberStart && this.scrubberEnd)
            d3.select("#bottom-scrubber").call(this.scrubberBrush.move, [this.scrubberStart, this.scrubberEnd]);

        this.scaleX.domain([0, this.spanManager.maxTime]);

        let threads = Object.keys(this.spanManager.threads)
            .sort()
            .map(k => this.spanManager.spans[this.spanManager.threads[k].id]);

        var yScaleMini = d3.scaleLinear()
            .domain([0, threads.length])
            .range([0, this.layoutScrubberHeight]);

        let minis = this.scrubberPanel.selectAll(".miniItems")
            .data(threads, d => { return d.id; })
            .attr("x", d => { return this.scaleX(d.start); })
            .attr("y", (d, i) => { return yScaleMini(i) - 5; })
            .attr("width", d => this.scaleX(this.spanEnd(d) - d.start));

        minis.enter().append("rect")
            .attr("class", d => { return "miniItems" })
            .attr("x", d => { return this.scaleX(d.start); })
            .attr("y", (d, i) => { return yScaleMini(i) - 5; })
            .attr("width", d => this.scaleX(this.spanEnd(d) - d.start))
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
        {Wakeup: {id: 100, waking_span: 3, parked_span: 4, ts: 310}},
        {AsyncOnCPU: {id: 4, ts: 320}},
        {AsyncOffCPU: {id: 4, ts: 330}},
        {AsyncEnd: {id: 3, ts: 420, outcome: "Success"}},
        {AsyncEnd: {id: 4, ts: 530, outcome: "Success"}},
        {AsyncStart: {name: "DownloadBlock", parent_id: 2, id: 5, ts: 550}},
        {AsyncStart: {name: "RemoteAdd(/foo)", parent_id: 1, id: 6, ts: 580}},
        {Wakeup: {id: 101, waking_span: 3, parked_span: 6, ts: 330}},
        {AsyncOnCPU: {id: 6, ts: 600}},
        {AsyncOffCPU: {id: 6, ts: 605}},
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
