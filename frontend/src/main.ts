class Span {
    public expanded: boolean;
    public id: number;
    constructor(readonly name: string, readonly start: number, readonly end: number, public children: Array<Span>) {
        this.expanded = true;
    }
}

var root = new Span("root-don't show", 0, 10000, [
    new Span("Scheduler", 0, 10000, [
        new Span("PreLocal", 265, 420, [
            new Span("Hash", 300, 405, []),
        ]),
        new Span("RemoteAdd(/foo)", 580, 615, []),
        new Span("RemoteAdd(/bar)", 620, 900, []),
        new Span("RemoteAdd(/baz)", 960, 1265, []),
        new Span("RemoteAdd(/bang)", 1270, 1360, []),
    ]),
    new Span("Downloader", 0, 10000, [
        new Span("DownloadBlock", 300, 530, []),
        new Span("DownloadBlock", 550, 700, []),
        new Span("DownloadBlock", 710, 790, []),
        new Span("DownloadBlock", 800, 1180, []),
        new Span("DownloadBlock", 1270, 1365, []),
        new Span("DownloadBlock", 1370, 1640, []),
        new Span("DownloadBlock", 1645, 1910, []),
    ]),
]);

const SPAN_HEIGHT = 80;
const MINI_SPAN_HEIGHT = 12; 

var hierarchy = d3.hierarchy(root, span => {
    if (span.expanded) {
        return span.children;
    } else {
        return [];
    }
});
let count: number = hierarchy.descendants().length;
console.log(count);

let next_id = 0;
hierarchy.descendants().forEach((node, idx) => {
    node.data.id = ++next_id;
});

console.log(hierarchy.descendants());

var timeBegin = 0;
var timeEnd = 10000;

var windowWidth = window.innerWidth - 10;
var windowHeight = window.innerHeight - 10;

// var m = [20, 15, 15, 120], //top right bottom left
//     w = windowWidth,
//     h = windowHeight - m[0] - m[2], // use constant here? 
//     miniHeight = count * 12 + 50, // use constant here?
//     mainHeight = h - miniHeight - 50;

let leftPadding = 100;
let mainHeight = windowHeight * 0.8;
let mainWidth = windowWidth - leftPadding;
let miniHeight = windowHeight * 0.2;

//scales
var x = d3.scaleLinear()
    .domain([timeBegin, timeEnd])
    .range([0, mainWidth]);
var x1 = d3.scaleLinear()
    .range([0, mainWidth]);
var yScale = d3.scaleLinear()
    .domain([0, count]) // this is with everything unexpanded
    .range([0, mainHeight]);
var yScaleMini = d3.scaleLinear()
    .domain([0, count]) // this is with everything unexpanded
    .range([0, miniHeight]);
// console.log(x1);

var chart = d3.select("body")
    .append("svg")
    .attr("width", windowWidth)
    .attr("height", windowHeight)
    .attr("class", "chart");

chart.append("defs").append("clipPath")
    .attr("id", "clip")
    .append("rect")
    .attr("width", windowWidth)
    .attr("height", mainHeight);

var main = chart.append("g")
    .attr("transform", "translate(" + leftPadding + "," + 0 + ")")
    .attr("width", windowWidth)
    .attr("height", mainHeight)
    .attr("class", "main");

var mini = chart.append("g")
    .attr("transform", "translate(" + leftPadding + "," + mainHeight + ")")
    .attr("width", windowWidth)
    .attr("height", miniHeight)
    .attr("class", "mini");

//main lanes and texts
// main.append("g").selectAll(".laneLines")
//     .data(items)
//     .enter().append("line")
//     .attr("x1", m[1])
//     .attr("yScale", function (d) { return yScale(d.lane); })
//     .attr("x2", w)
//     .attr("yScaleMini", function (d) { return yScale(d.lane); })
//     .attr("stroke", "lightgray")

// hierarchies.forEach((hierarchy, idx) => {
    
// });

// lane text...
// main.append("g").selectAll(".laneText")
//     .data(hierarchy.descendants())
//     .enter().append("text")
//     .text(function (d) { return d.data.name; })
//     .attr("x", -m[1])
//     .attr("y", function (d, i) { return yScale(i + .5); })
//     .attr("dy", ".5ex")
//     .attr("text-anchor", "end")
//     .attr("class", "laneText");

//mini lanes and texts
// mini.append("g").selectAll(".laneLines")
//     .data(items)
//     .enter().append("line")
//     .attr("x1", m[1])
//     .attr("yScale", function (d) { return yScaleMini(d.lane); })
//     .attr("x2", w)
//     .attr("yScaleMini", function (d) { return yScaleMini(d.lane); })
//     .attr("stroke", "lightgray");

// mini.append("g").selectAll(".laneText")
//     .data(lanes)
//     .enter().append("text")
//     .text(function (d) { return d.name; })
//     .attr("x", -m[1])
//     .attr("y", function (d, i) { return yScaleMini(i + .5); })
//     .attr("dy", ".5ex")
//     .attr("text-anchor", "end")
//     .attr("class", "laneText");

var itemRects = main.append("g")
    .attr("clip-path", "url(#clip)");

// Compute the layout.
let map = {};
let index = -1;
console.log(hierarchy.descendants());
hierarchy.eachBefore(n => {
    map[n.data.id] = {
        rowIdx: ++index
    }
})
console.log(map);

//mini item rects
mini.append("g")
    .selectAll("miniItems") // why do we filter by miniItems? what does this do?
    .data(hierarchy.descendants())
    .enter().append("rect")
    .attr("class", function (d) { return "miniItem" + d.data.name; })
    .attr("x", function (d) { return x(d.data.start); })
    .attr("y", function (n) { 
        console.log("1querying " + n.data.id);
        return yScaleMini(map[n.data.id].rowIdx) - 5; })
    .attr("width", function (d) { return x(d.data.end - d.data.start); })
    .attr("height", 10);

//mini labels
// mini.append("g").selectAll(".miniLabels")
//     .data(items)
//     .enter().append("text")
//     .text(function (d) { return d.id; })
//     .attr("x", function (d) { return x(d.start); })
//     .attr("y", function (d) { return yScaleMini(d.lane + .5); })
//     .attr("dy", ".5ex");


var brush = d3.brushX()
    .extent([[0, 0], [windowWidth - leftPadding, miniHeight]])
    // .x(x)
    .on("brush", display);

mini.append("g")
    .attr("class", "x brush")
    .call(brush)
    .selectAll("rect")
    .attr("y", 0)
    .attr("height", miniHeight);

// mini.append("g")
//     .attr("class", "brush");
// .call(brush);
// .call(brush.move, [[307, 167], [611, 539]]);

// display();

var minExtent;
var maxExtent;

function display() {
    console.log(d3.event);
    // var labels,
    //     minExtent = brush.extent()[0],
    //     maxExtent = brush.extent()[1];

    if (d3.event.selection !== undefined) {
        // move this to only fire on brush moves
        minExtent = d3.event.selection.map(x.invert)[0];
        maxExtent = d3.event.selection.map(x.invert)[1];
    }

    // console.log(d3.event.selection);
    // is this bad to be recomputing the hierachy here?
    var hierarchy = d3.hierarchy(root, span => {
        if (span.expanded) {
            return span.children;
        } else {
            console.log("NOT EXPANDED");
            return [];
        }
    });

    var visItems = hierarchy.descendants().filter(d => {
        return d.data.start < maxExtent && d.data.end > minExtent;
    });

    console.log("Visible items: " + visItems.length);
    // console.log(d3.event.selection.map(x.invert));
    // mini.select(".brush")
    //     .call(brush.extent([minExtent, maxExtent]));

    x1.domain([minExtent, maxExtent]);

    //update main item rects
    // For already-visible spans, make sure they're sized appropriately.
    let rects = itemRects.selectAll("rect")
        .data(visItems, (d: any) => { return d.data.id; })
        .attr("x", function (d) { return x1(d.data.start); })
        .attr("width", function (d) { return x1(d.data.end) - x1(d.data.start); })
        .on("click", function(node) { // we should set this up once at the beginning
            console.log("got clicked: " + node.data.name);
            node.data.expanded = !node.data.expanded;
            display();
        });

    // For new entries, do the things.
    let newRects = rects.enter().append("rect")
        .attr("class", function (d) { return "miniItem" + d.data.name; })
        .attr("x", function (d) { return x1(d.data.start); })
        .attr("y", function (d) { return yScale(map[d.data.id].rowIdx) - 100; })
        .attr("width", function (d) { return x1(d.data.end) - x1(d.data.start); })
        .attr("height", function (d) { return .8 * yScale(1); })
        .style("opacity", 0.5);
    
    newRects.transition()
        .duration(200)
        .style("opacity", 1)
        .attr("y", function (d) { return yScale(map[d.data.id].rowIdx) + 10; });

    rects.exit().remove();

    // same deal w/ the text
    var labels = itemRects.selectAll("text")
        .data(visItems, (d: any) => { return d.data.id; })
        .attr("x", function (d) { return x1(Math.max(d.data.start, minExtent)); });

    labels.enter().append("text")
        .text(function (d) { return d.data.name; })
        // .attr("class", "span-text") // why doesn't this work? why inline fill???
        .style("fill", "white")
        .attr("x", function (d) { return x1(Math.max(d.data.start, minExtent)); })
        .attr("y", function (d) { return yScale(map[d.data.id].rowIdx) + 20; })
        .attr("text-anchor", "start");

    labels.exit().remove();

}

// class Startup {
//     public static main(): number {
//         console.log('Hello World');
//         return 0;
//     }
// }

// Startup.main();

document.addEventListener('mousedown', function(e){ e.preventDefault(); }, false);
