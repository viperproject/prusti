# Any copyright is dedicated to the Public Domain.
# http://creativecommons.org/publicdomain/zero/1.0/

from nagini_contracts.contracts import *
from nagini_contracts.io_contracts import *
from typing import List, Set


class Student:
    def __init__(self, name: str) -> None:
        Ensures(Acc(self.name) and self.name == name and self.undecided())
        self.name = name  # type: str
        self.courses = []  # type: List[str]
        Fold(self.undecided())

    def enroll(self, course_name: str) -> None:
        Requires(self.undecided())
        Ensures(self.enrolled(course_name))

        Unfold(self.undecided())
        self.courses.append(course_name)
        Fold(self.enrolled(course_name))

    @Predicate
    def enrolled(self, course_name: str) -> bool:
        return Acc(self.courses) and Acc(
            list_pred(self.courses)) and course_name in self.courses

    @Predicate
    def undecided(self) -> bool:
        return Acc(self.courses) and Acc(list_pred(self.courses))


class GradStudent(Student):
    def __init__(self, name: str, advisor_name: str) -> None:
        Ensures(Acc(
            self.name) and self.name == name and self.undecided())  # type: ignore
        Ensures(Acc(
            self.advisor_name) and self.advisor_name == advisor_name)  # type: ignore
        super().__init__(name)
        self.advisor_name = advisor_name
        self.research_only = True
        Fold(self.undecided())

    def enroll(self, course_name: str) -> None:
        Requires(self.undecided())
        Ensures(self.enrolled(course_name))

        Unfold(self.undecided())
        self.courses.append(course_name)
        self.research_only = False
        Fold(self.enrolled(course_name))

    @Predicate
    def enrolled(self, course_name: str) -> bool:
        return Acc(self.research_only) and not self.research_only

    @Predicate
    def undecided(self) -> bool:
        return Acc(self.research_only) and self.research_only


def enroll_all(students: Set[Student], course_name: str) -> None:
    Requires(Acc(set_pred(students), 1 / 2) and
             Forall(students, lambda s: (s.undecided(), [])))
    Ensures(Acc(set_pred(students), 1 / 2) and
            Forall(students, lambda s: (s.enrolled(course_name), [])))
    for student in students:
        Invariant(Forall(students, lambda s: (
        Implies(s not in Previous(student), s.undecided()), [])) and
                  Forall(Previous(student),
                         lambda s: (s.enrolled(course_name), [[s in students]])))
        student.enroll(course_name)


def client() -> None:
    s1 = Student('Marc')
    course = 'COOP'
    enroll_all({s1}, course)
    Unfold(s1.enrolled(course))
    assert course in s1.courses
    #:: ExpectedOutput(assert.failed:assertion.false)
    assert 'sae' in s1.courses
