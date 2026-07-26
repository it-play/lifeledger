#!/usr/bin/env python3
"""Regenerate the immutable KRX closure snapshot with Python's standard library."""

from __future__ import annotations

import csv
from dataclasses import dataclass
from datetime import date, timedelta
from pathlib import Path


CALENDAR_ID = "krx-equity-2026-rules-v1"
COVERAGE_START = date(2026, 1, 1)
COVERAGE_END = date(2060, 12, 31)
VERIFIED_YEAR = 2026
OUTPUT_PATH = Path(__file__).with_name("krx_equity_2026_rules_v1.csv")

# Anchors are immutable inputs to this snapshot. A later official correction creates a new
# calendar version instead of rewriting these values.
LUNAR_ANCHORS = {
    2026: ("2026-02-17", "2026-05-24", "2026-09-25"),
    2027: ("2027-02-07", "2027-05-13", "2027-09-15"),
    2028: ("2028-01-27", "2028-05-02", "2028-10-03"),
    2029: ("2029-02-13", "2029-05-20", "2029-09-22"),
    2030: ("2030-02-03", "2030-05-09", "2030-09-12"),
    2031: ("2031-01-23", "2031-05-28", "2031-10-01"),
    2032: ("2032-02-11", "2032-05-16", "2032-09-19"),
    2033: ("2033-01-31", "2033-05-06", "2033-09-08"),
    2034: ("2034-02-19", "2034-05-25", "2034-09-27"),
    2035: ("2035-02-08", "2035-05-15", "2035-09-16"),
    2036: ("2036-01-28", "2036-05-03", "2036-10-04"),
    2037: ("2037-02-15", "2037-05-22", "2037-09-24"),
    2038: ("2038-02-04", "2038-05-11", "2038-09-13"),
    2039: ("2039-01-24", "2039-04-30", "2039-10-02"),
    2040: ("2040-02-12", "2040-05-18", "2040-09-21"),
    2041: ("2041-02-01", "2041-05-07", "2041-09-10"),
    2042: ("2042-01-22", "2042-05-26", "2042-09-28"),
    2043: ("2043-02-10", "2043-05-16", "2043-09-17"),
    2044: ("2044-01-30", "2044-05-05", "2044-10-05"),
    2045: ("2045-02-17", "2045-05-24", "2045-09-25"),
    2046: ("2046-02-06", "2046-05-13", "2046-09-15"),
    2047: ("2047-01-26", "2047-05-02", "2047-10-04"),
    2048: ("2048-02-14", "2048-05-20", "2048-09-22"),
    2049: ("2049-02-02", "2049-05-09", "2049-09-11"),
    2050: ("2050-01-23", "2050-05-28", "2050-09-30"),
    2051: ("2051-02-11", "2051-05-17", "2051-09-19"),
    2052: ("2052-02-01", "2052-05-06", "2052-09-07"),
    2053: ("2053-02-19", "2053-05-25", "2053-09-26"),
    2054: ("2054-02-08", "2054-05-15", "2054-09-16"),
    2055: ("2055-01-28", "2055-05-04", "2055-10-05"),
    2056: ("2056-02-15", "2056-05-22", "2056-09-24"),
    2057: ("2057-02-04", "2057-05-11", "2057-09-13"),
    2058: ("2058-01-24", "2058-04-30", "2058-10-02"),
    2059: ("2059-02-12", "2059-05-19", "2059-09-21"),
    2060: ("2060-02-02", "2060-05-07", "2060-09-09"),
}


@dataclass(frozen=True)
class Holiday:
    dates: tuple[date, ...]
    label: str
    substitute_mode: str | None = None


@dataclass(frozen=True, order=True)
class Closure:
    day: date
    provenance: str
    label: str


def parse_anchor(raw: str) -> date:
    return date.fromisoformat(raw)


def holidays_for_year(year: int) -> list[Holiday]:
    lunar_new_year, buddhas_birthday, chuseok = map(parse_anchor, LUNAR_ANCHORS[year])
    one_day = timedelta(days=1)
    holidays = [
        Holiday((date(year, 1, 1),), "신정"),
        Holiday(
            (lunar_new_year - one_day, lunar_new_year, lunar_new_year + one_day),
            "설날",
            "blockSundayOrOverlap",
        ),
        Holiday((date(year, 3, 1),), "삼일절", "weekendOrOverlap"),
        Holiday((date(year, 5, 5),), "어린이날", "weekendOrOverlap"),
        Holiday((buddhas_birthday,), "부처님오신날", "weekendOrOverlap"),
        Holiday((date(year, 6, 6),), "현충일"),
        Holiday((date(year, 8, 15),), "광복절", "weekendOrOverlap"),
        Holiday(
            (chuseok - one_day, chuseok, chuseok + one_day),
            "추석",
            "blockSundayOrOverlap",
        ),
        Holiday((date(year, 10, 3),), "개천절", "weekendOrOverlap"),
        Holiday((date(year, 10, 9),), "한글날", "weekendOrOverlap"),
        Holiday((date(year, 12, 25),), "기독탄신일", "weekendOrOverlap"),
    ]
    if year == VERIFIED_YEAR:
        holidays.append(Holiday((date(2026, 6, 3),), "제9회 전국동시지방선거"))
    return holidays


def statutory_closures(year: int) -> list[tuple[date, str]]:
    holidays = holidays_for_year(year)
    closures = [
        (holiday_date, holiday.label)
        for holiday in holidays
        for holiday_date in holiday.dates
    ]
    closed_dates = {holiday_date for holiday_date, _ in closures}
    for component in overlapping_components(holidays):
        component_dates = {
            holiday_date for holiday in component for holiday_date in holiday.dates
        }
        overlaps = len(component) > 1
        requires_substitute = False
        for holiday in component:
            if holiday.substitute_mode is None:
                continue
            if holiday.substitute_mode == "weekendOrOverlap":
                requires_substitute |= overlaps or any(
                    holiday_date.weekday() >= 5 for holiday_date in holiday.dates
                )
            else:
                requires_substitute |= overlaps or any(
                    holiday_date.weekday() == 6 for holiday_date in holiday.dates
                )
        if not requires_substitute:
            continue

        substitute = max(component_dates) + timedelta(days=1)
        while substitute.weekday() >= 5 or substitute in closed_dates:
            substitute += timedelta(days=1)
        labels = "·".join(dict.fromkeys(holiday.label for holiday in component))
        closures.append((substitute, f"대체공휴일({labels})"))
        closed_dates.add(substitute)

    return closures


def overlapping_components(holidays: list[Holiday]) -> list[list[Holiday]]:
    remaining = list(holidays)
    components: list[list[Holiday]] = []
    while remaining:
        component = [remaining.pop(0)]
        component_dates = set(component[0].dates)
        changed = True
        while changed:
            changed = False
            for holiday in list(remaining):
                if component_dates.isdisjoint(holiday.dates):
                    continue
                remaining.remove(holiday)
                component.append(holiday)
                component_dates.update(holiday.dates)
                changed = True
        components.append(component)
    return components


def projected_year_end_closure(year: int, public_closures: set[date]) -> date:
    closure = date(year, 12, 31)
    while closure.weekday() >= 5 or closure in public_closures:
        closure -= timedelta(days=1)
    return closure


def generate_closures() -> list[Closure]:
    closures: list[Closure] = []
    for year in range(COVERAGE_START.year, COVERAGE_END.year + 1):
        provenance = "krxPublished" if year == VERIFIED_YEAR else "statutoryProjected"
        public = statutory_closures(year)
        closures.extend(Closure(day, provenance, label) for day, label in public)

        rule_provenance = "krxPublished" if year == VERIFIED_YEAR else "krxRuleProjected"
        closures.append(Closure(date(year, 5, 1), rule_provenance, "근로자의 날"))
        year_end = projected_year_end_closure(year, {day for day, _ in public})
        closures.append(Closure(year_end, rule_provenance, "연말 휴장일"))

    return sorted(set(closures))


def write_snapshot(closures: list[Closure]) -> None:
    metadata = [
        f"# calendarId={CALENDAR_ID}",
        f"# coverageStart={COVERAGE_START.isoformat()}",
        f"# coverageEnd={COVERAGE_END.isoformat()}",
        "# verifiedThrough=2026-12-31",
        "# projectedRange=2027-01-01/2060-12-31",
        "# ruleSetVersion=kr-public-holidays-and-krx-closures-2026-v1",
        "# lunarAnchorVersion=korean-lunisolar-anchors-2026-2060-v1",
        "# sourceKrx=https://global.krx.co.kr/contents/GLB/06/0602/0602010201/GLB0602010201T1.jsp",
        "# sourceKasi=https://www.kasi.re.kr/kor/post/newsMaterial/32031",
        "# sourceNec=https://www.nec.go.kr/common/board/Download.do?bcIdx=227983&cbIdx=1129&streFileNm=4d70b48b-4072-4daa-b2fd-4f674c542639.pdf",
        "# generatedBy=generate_krx_calendar.py",
    ]
    with OUTPUT_PATH.open("w", encoding="utf-8", newline="") as output:
        output.write("\n".join(metadata))
        output.write("\n")
        writer = csv.writer(output, lineterminator="\n")
        writer.writerow(("date", "provenance", "label"))
        for closure in closures:
            writer.writerow((closure.day.isoformat(), closure.provenance, closure.label))


if __name__ == "__main__":
    write_snapshot(generate_closures())
