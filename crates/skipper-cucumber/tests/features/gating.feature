Feature: Skipper gating

  Scenario: runs when not in the sheet
    Given a counter
    When it is incremented
    Then it is 1

  Scenario: disabled in the e2e check
    Given a counter
    Then the scenario must have been skipped
