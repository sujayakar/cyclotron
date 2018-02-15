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
    private maxTime;

    constructor() {
        // First some global configuration.
        document.addEventListener(
            'mousedown',
            e => e.preventDefault(),
            false
        );

        this.spanManager = new SpanManager();
        test_events(this.spanManager);

        const SPAN_HEIGHT = 80;
        const MINI_SPAN_HEIGHT = 12;

        var hierarchy = this.nodes();
        let count: number = hierarchy.descendants().length;
        console.log(count);
        console.log(hierarchy.descendants());

        var timeBegin = 0;
        this.maxTime = 10000;

        var windowWidth = window.innerWidth - 10;
        var windowHeight = window.innerHeight - 10;

        let leftPadding = 100;
        let mainHeight = windowHeight * 0.75;
        this.layoutMainHeight = mainHeight;
        let mainWidth = windowWidth - leftPadding;
        this.layoutMainWidth = mainWidth;
        let miniHeight = windowHeight * 0.2;
        this.layoutScrubberHeight = miniHeight;
        let timeHeight = windowHeight * 0.05;

        //scales
        var x = d3.scaleLinear()
            .domain([timeBegin, this.maxTime])
            .range([0, mainWidth]);
        this.scaleX = x;
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

        this.drawScrubber();
    }

    private nodes(expanded_only = false) {
        let root = new Root(this.spanManager, this.maxTime);
        return d3.hierarchy(root, span => span.getChildren(!expanded_only));
    }

    private drawMain() {
        // Update the axis at the top.
        let axisScale = d3.scaleTime()
            .domain([new Date(0, 0, 0, 0, 0, this.scrubberStart), new Date(0, 0, 0, 0, 0, this.scrubberEnd)])
            .range([0, this.layoutMainWidth]);
        this.topAxis.call(d3.axisBottom(axisScale).tickFormat((d: any) => {
            return d3.timeFormat("%Mm %Ss")(d);
        }));

        let hierarchy = this.nodes(true);

        let visItems = hierarchy.descendants().filter(d => {
            return d.data.intersects(this.scrubberStart, this.scrubberEnd);
        });

        // Compute a new order based on what's visible.
        let map = {};
        let index = -1;
        hierarchy.eachBefore(n => {
            if (n.data.intersects(this.scrubberStart, this.scrubberEnd)) {
                map[n.data.id] = {
                    rowIdx: ++index
                }
            }
        })

        console.log("Visible items: " + visItems.length);
        var x1 = d3.scaleLinear().range([0, this.layoutMainWidth]);
        x1.domain([this.scrubberStart, this.scrubberEnd]);

        // This scales all the spans to share the vertical space when they're fully expanded.
        //
        // We might want to use a fixed height here and scroll instead.
        var yScale = d3.scaleLinear()
            .domain([0, this.nodes().descendants().length])
            .range([0, this.layoutMainHeight]);

        let clickHandler = node => { // we should set this up once at the beginning
            console.log("got clicked: " + node.data.name);
            node.data.expanded = !node.data.expanded;
            this.drawMain();
        };

        let xPosition = d => { return x1(d.data.start); };

        // For already-visible spans, make sure they're positioned appropriately.
        //
        // Note that we animate changes on the y-axis, but not on the x-axis. This is so
        // that when you scroll side-to-side, things update immediately.
        let rects = this.mainPanel.selectAll("rect") // formerly itemRects
            .data(visItems, (d: any) => { return d.data.id; })
            .attr("x", xPosition)
            .attr("width", function (d) { return x1(d.data.end) - x1(d.data.start); })
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
            .attr("y", function (d) { return yScale(map[d.data.id].rowIdx); });

        // For new entries, do the things.
        let newRects = rects.enter().append("rect")
            .attr("class", function (d) { return "span"; })
            .attr("x", xPosition)
            .attr("y", function (d) { return yScale(map[d.data.id].rowIdx); })
            .attr("width", function (d) { return x1(d.data.end) - x1(d.data.start); })
            .attr("height", function (d) { return .8 * yScale(1); })
            .attr("rx", 10)
            .attr("ry", 10)
            .on("click", clickHandler)
            .style("opacity", 0.7);

        newRects.transition()
            .duration(150)
            .style("opacity", 1.0)
            .attr("y", function (d) { return yScale(map[d.data.id].rowIdx); });

        rects.exit().remove();

        // same deal w/ the text
        var labels = this.mainPanel.selectAll("text") // formerly itemRects.selectAll("text")
            .data(visItems, (d: any) => { return d.data.id; })
            .attr("x", d => { return x1(Math.max(d.data.start, this.scrubberStart)); })
            .attr("y", d => { return yScale(map[d.data.id].rowIdx) + 20; });

        labels.enter().append("text")
            .text(d => { return d.data.name; })
            .attr("class", "span-text") // why doesn't this work? why inline fill???
            .attr("x", xPosition)
            .attr("y", d => { return yScale(map[d.data.id].rowIdx) + 20; })
            .attr("text-anchor", "start");

        labels.exit().remove();

    }

    private drawScrubber() {
        // Compute the layout.
        let map = {};
        let index = -1;
        let hierarchy = this.nodes();
        hierarchy.eachBefore(n => {
            map[n.data.id] = {
                rowIdx: ++index
            }
        })
        console.log(map);

        let count: number = hierarchy.descendants().length;

        // In the scrubber we always show everything expanded (for now).
        var yScaleMini = d3.scaleLinear()
            .domain([0, count])
            .range([0, this.layoutScrubberHeight]);

        //mini item rects
        this.scrubberPanel.append("g")
            .selectAll("miniItems") // why do we filter by miniItems? what does this do?
            .data(hierarchy.descendants())
            .enter().append("rect")
            .attr("class", d => { return "miniItem" + d.data.name; })
            .attr("x", d => { return this.scaleX(d.data.start); })
            .attr("y", function (n) {
                return yScaleMini(map[n.data.id].rowIdx) - 5; })
            .attr("width", d => { return this.scaleX(d.data.end - d.data.start); })
            .attr("height", 10);

        var brush = d3.brushX()
            .extent([[0, 0], [this.layoutMainWidth, this.layoutScrubberHeight]])
            .on("brush", () => {
                this.scrubberStart = d3.event.selection.map(this.scaleX.invert)[0];
                this.scrubberEnd = d3.event.selection.map(this.scaleX.invert)[1];
                this.drawMain();
            });

        this.scrubberPanel.append("g")
            .attr("class", "x brush")
            .call(brush)
            .selectAll("rect")
            .attr("y", 0)
            .attr("height", this.layoutScrubberHeight);
    }
}

new Cyclotron();

function test_events(manager) {
    let events = [
        {ThreadStart: {name: "Control", id: 0, ts: 0}},
        {AsyncStart: {name: "Scheduler", parent_id: 0, id: 1, ts: 0}},
        {AsyncStart: {name: "Downloader", parent_id: 0, id: 2, ts: 0}},
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
        {AsyncEnd: {id: 1, ts: 10000, outcome: "Success"}},
        {AsyncEnd: {id: 2, ts: 10000, outcome: "Success"}},
        {ThreadEnd: {id: 0, ts: 10000}},
    ];
    events.forEach(function (e) {manager.addEvent(e)});
}
