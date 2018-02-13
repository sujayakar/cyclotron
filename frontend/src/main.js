// import * as d3 from 'd3';
var Lane = /** @class */ (function () {
    function Lane(id, parent_id, name) {
        this.id = id;
        this.parent_id = parent_id;
        this.name = name;
    }
    return Lane;
}());
var lanes = [
    new Lane(0, null, "Scheduler"),
    new Lane(1, null, "Downloader"),
    new Lane(2, null, "Uploader"),
    new Lane(3, null, "Protocol"),
];
var laneLength = lanes.length;
var Span = /** @class */ (function () {
    function Span(lane, id, start, end) {
        this.lane = lane;
        this.id = id;
        this.start = start;
        this.end = end;
    }
    return Span;
}());
var items = [
    new Span(0, "Qin", 5, 205),
    new Span(0, "Jin", 265, 420),
    new Span(0, "Sui", 580, 615),
    new Span(0, "Tang", 620, 900),
    new Span(0, "Song", 960, 1265),
    new Span(0, "Yuan", 1270, 1365),
    new Span(0, "Ming", 1370, 1640),
    new Span(0, "Qing", 1645, 1910),
    new Span(1, "Yamato", 300, 530),
    new Span(1, "Asuka", 550, 700),
    new Span(1, "Nara", 710, 790),
    new Span(1, "Heian", 800, 1180),
    new Span(1, "Kamakura", 1190, 1330),
    new Span(1, "Muromachi", 1340, 1560),
    new Span(1, "Edo", 1610, 1860),
    new Span(1, "Meiji", 1870, 1900),
    new Span(1, "Taisho", 1910, 1920),
    new Span(1, "Showa", 1925, 1985),
    new Span(1, "Heisei", 1990, 1995),
    new Span(2, "Goryeo", 920, 1380),
    new Span(2, "Joseon", 1390, 1890),
    new Span(3, "Qin", 5, 205),
    new Span(3, "Jin", 265, 420),
    new Span(3, "Sui", 580, 615),
];
// for (let i of Array.from(Array(10000).keys())) {
//     items.push(new Span(3, "Tang", 700 +  "e),
// }
var timeBegin = 0;
var timeEnd = 10000;
var windowWidth = window.innerWidth;
var m = [20, 15, 15, 120], //top right bottom left
w = windowWidth - m[1] - m[3], h = 500 - m[0] - m[2], miniHeight = laneLength * 12 + 50, mainHeight = h - miniHeight - 50;
//scales
var x = d3.scaleLinear()
    .domain([timeBegin, timeEnd])
    .range([0, w]);
var x1 = d3.scaleLinear()
    .range([0, w]);
var y1 = d3.scaleLinear()
    .domain([0, laneLength])
    .range([0, mainHeight]);
var y2 = d3.scaleLinear()
    .domain([0, laneLength])
    .range([0, miniHeight]);
// console.log(x1);
var chart = d3.select("body")
    .append("svg")
    .attr("width", w + m[1] + m[3])
    .attr("height", h + m[0] + m[2])
    .attr("class", "chart");
chart.append("defs").append("clipPath")
    .attr("id", "clip")
    .append("rect")
    .attr("width", w)
    .attr("height", mainHeight);
var main = chart.append("g")
    .attr("transform", "translate(" + m[3] + "," + m[0] + ")")
    .attr("width", w)
    .attr("height", mainHeight)
    .attr("class", "main");
var mini = chart.append("g")
    .attr("transform", "translate(" + m[3] + "," + (mainHeight + m[0]) + ")")
    .attr("width", w)
    .attr("height", miniHeight)
    .attr("class", "mini");
//main lanes and texts
// main.append("g").selectAll(".laneLines")
//     .data(items)
//     .enter().append("line")
//     .attr("x1", m[1])
//     .attr("y1", function (d) { return y1(d.lane); })
//     .attr("x2", w)
//     .attr("y2", function (d) { return y1(d.lane); })
//     .attr("stroke", "lightgray")
main.append("g").selectAll(".laneText")
    .data(lanes)
    .enter().append("text")
    .text(function (d) { return d.name; })
    .attr("x", -m[1])
    .attr("y", function (d, i) { return y1(i + .5); })
    .attr("dy", ".5ex")
    .attr("text-anchor", "end")
    .attr("class", "laneText");
//mini lanes and texts
mini.append("g").selectAll(".laneLines")
    .data(items)
    .enter().append("line")
    .attr("x1", m[1])
    .attr("y1", function (d) { return y2(d.lane); })
    .attr("x2", w)
    .attr("y2", function (d) { return y2(d.lane); })
    .attr("stroke", "lightgray");
mini.append("g").selectAll(".laneText")
    .data(lanes)
    .enter().append("text")
    .text(function (d) { return d.name; })
    .attr("x", -m[1])
    .attr("y", function (d, i) { return y2(i + .5); })
    .attr("dy", ".5ex")
    .attr("text-anchor", "end")
    .attr("class", "laneText");
var itemRects = main.append("g")
    .attr("clip-path", "url(#clip)");
//mini item rects
mini.append("g").selectAll("miniItems")
    .data(items)
    .enter().append("rect")
    .attr("class", function (d) { return "miniItem" + d.lane; })
    .attr("x", function (d) { return x(d.start); })
    .attr("y", function (d) { return y2(d.lane + .5) - 5; })
    .attr("width", function (d) { return x(d.end - d.start); })
    .attr("height", 10);
//mini labels
// mini.append("g").selectAll(".miniLabels")
//     .data(items)
//     .enter().append("text")
//     .text(function (d) { return d.id; })
//     .attr("x", function (d) { return x(d.start); })
//     .attr("y", function (d) { return y2(d.lane + .5); })
//     .attr("dy", ".5ex");
//brush
var brush = d3.brushX()
    .extent([[0, 0], [w, miniHeight]])
    .on("brush", display);
mini.append("g")
    .attr("class", "x brush")
    .call(brush)
    .selectAll("rect")
    .attr("y", 1)
    .attr("height", miniHeight - 1);
// mini.append("g")
//     .attr("class", "brush");
// .call(brush);
// .call(brush.move, [[307, 167], [611, 539]]);
// display();
function display() {
    // console.log(d3.event);
    // var rects, labels,
    //     minExtent = brush.extent()[0],
    //     maxExtent = brush.extent()[1];
    var minExtent = d3.event.selection.map(x.invert)[0];
    var maxExtent = d3.event.selection.map(x.invert)[1];
    // console.log(d3.event.selection);
    var visItems = items.filter(function (d) {
        return d.start < maxExtent && d.end > minExtent;
    });
    console.log("Visible items: " + visItems.length);
    // console.log(d3.event.selection.map(x.invert));
    // mini.select(".brush")
    //     .call(brush.extent([minExtent, maxExtent]));
    x1.domain([minExtent, maxExtent]);
    //update main item rects
    var rects = itemRects.selectAll("rect")
        .data(visItems, function (d) { return d.id; })
        .attr("x", function (d) { return x1(d.start); })
        .attr("width", function (d) { return x1(d.end) - x1(d.start); })
        .on("click", function () {
        console.log("got clicked");
    });
    rects.enter().append("rect")
        .attr("class", function (d) { return "miniItem" + d.lane; })
        .attr("x", function (d) { return x1(d.start); })
        .attr("y", function (d) { return y1(d.lane) + 10; })
        .attr("width", function (d) { return x1(d.end) - x1(d.start); })
        .attr("height", function (d) { return .8 * y1(1); });
    rects.exit().remove();
    //update the item labels
    var labels = itemRects.selectAll("text")
        .data(visItems, function (d) { return d.id; })
        .attr("x", function (d) { return x1(Math.max(d.start, minExtent) + 2); });
    labels.enter().append("text")
        .text(function (d) { return d.id; })
        .attr("x", function (d) { return x1(Math.max(d.start, minExtent)); })
        .attr("y", function (d) { return y1(d.lane + .5); })
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
