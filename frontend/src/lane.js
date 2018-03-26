"use strict";
exports.__esModule = true;
var Lane = /** @class */ (function () {
    function Lane(id, index, span) {
        this.id = id;
        this.index = index;
        this.spans = [span];
    }
    Lane.prototype.isOpen = function () {
        return this.lastSpan().isOpen();
    };
    Lane.prototype.lastSpan = function () {
        return this.spans[this.spans.length - 1];
    };
    Lane.prototype.push = function (span) {
        var last = this.lastSpan();
        if (last.isOpen()) {
            return false;
        }
        if (last.end > span.start) {
            return false;
        }
        if (last.start > span.start) {
            throw new Error("Out of order span " + span);
        }
        this.spans.push(span);
        return true;
    };
    return Lane;
}());
exports.Lane = Lane;
