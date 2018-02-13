//data
var lanes = ["Chinese", "Japanese", "Korean", "Another"];
var laneLength = lanes.length;
var items = [
    { "lane": 0, "id": "Qin", "start": 5, "end": 205 },
    { "lane": 0, "id": "Jin", "start": 265, "end": 420 },
    { "lane": 0, "id": "Sui", "start": 580, "end": 615 },
    { "lane": 0, "id": "Tang", "start": 620, "end": 900 },
    { "lane": 0, "id": "Song", "start": 960, "end": 1265 },
    { "lane": 0, "id": "Yuan", "start": 1270, "end": 1365 },
    { "lane": 0, "id": "Ming", "start": 1370, "end": 1640 },
    { "lane": 0, "id": "Qing", "start": 1645, "end": 1910 },
    { "lane": 1, "id": "Yamato", "start": 300, "end": 530 },
    { "lane": 1, "id": "Asuka", "start": 550, "end": 700 },
    { "lane": 1, "id": "Nara", "start": 710, "end": 790 },
    { "lane": 1, "id": "Heian", "start": 800, "end": 1180 },
    { "lane": 1, "id": "Kamakura", "start": 1190, "end": 1330 },
    { "lane": 1, "id": "Muromachi", "start": 1340, "end": 1560 },
    { "lane": 1, "id": "Edo", "start": 1610, "end": 1860 },
    { "lane": 1, "id": "Meiji", "start": 1870, "end": 1900 },
    { "lane": 1, "id": "Taisho", "start": 1910, "end": 1920 },
    { "lane": 1, "id": "Showa", "start": 1925, "end": 1985 },
    { "lane": 1, "id": "Heisei", "start": 1990, "end": 1995 },
    { "lane": 2, "id": "Three Kingdoms", "start": 10, "end": 670 },
    { "lane": 2, "id": "North and South States", "start": 690, "end": 900 },
    { "lane": 2, "id": "Goryeo", "start": 920, "end": 1380 },
    { "lane": 2, "id": "Joseon", "start": 1390, "end": 1890 },
    { "lane": 2, "id": "Korean Empire", "start": 1900, "end": 1945 },
    { "lane": 3, "id": "Qin", "start": 5, "end": 205 },
    { "lane": 3, "id": "Jin", "start": 265, "end": 420 },
    { "lane": 3, "id": "Sui", "start": 580, "end": 615 },
];
// for (let i of Array.from(Array(10000).keys())) {
//     items.push({ "lane": 3, "id": "Tang", "start": 700 + i * 15, "end": 710 + i * 15 });
// }
var timeBegin = 0;
var timeEnd = 10000;

var windowWidth = window.innerWidth;
var m = [20, 15, 15, 120], //top right bottom left
    w = windowWidth - m[1] - m[3],
    h = 500 - m[0] - m[2],
    miniHeight = laneLength * 12 + 50,
    mainHeight = h - miniHeight - 50;

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
    .text(function (d) { return d; })
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
    .text(function (d) { return d; })
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
    // .x(x)
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
        .attr("width", function (d) { return x1(d.end) - x1(d.start); });

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

// var Startup = /** @class */ (function () {
//     function Startup() {
//     }
//     Startup.main = function () {
//         console.log('Hello World');
//         return 0;
//     };
//     return Startup;
// }());
// Startup.main();
//# sourceMappingURL=main.js.map
