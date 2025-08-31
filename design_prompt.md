Today we are going to start designing a Rust library called 'cella' designed for running cellular automata calculations.  The project will consist of a library which will provide the backend and API for the underlying implementation, and a binary crate used for testing the library and providing examples.  The functionality for each will be described in turn.

## Library Design
The library should support 1D and 2D cellular automata calculations.  Both types should use a similar architecture consisting of:
 * A `Grid` that contains all of the `Cells`.  The `Grid` should provide API that allows a developer to get information about the current "step" of a calculation defined in a `GridState`.  There should be functionality to load a `GridState` into the grid and resume a calculation (this means the grid and the containing objects must have serialization methods).
 * A `Cell` will represent an individual point in the grid.  It will contain a `CellState` object that encompasses information about it, what state it is in, how long it has been in that state, a truncated history of states is have had (maybe start with the last 5 states, but this should be controllable via some property or data file setting.)  The `Cell` will also contain the `Rule` which defines how the `Cell` updates itself.
 * A `Rule` contains the information required to update a `Cell` to a new state.  The `Rule` needs to provide API that allows the grid to ask what information it needs about it's neighborhood to update and a corresponding API to accept in that neighborhood and update the `Cell` state as needed.  More detail about the design of the `Rule`s can be found below.

### Types of Rules

The types of `Rule` is specific to the dimension of the corresponding `Grid`.  The `Rule` encompasses all the logic for determining what neighborhood is needed for the `Cell` to update its state as well and for taking a neighborhood and computing a new state.

All rule are defined by a list of subrules of the form: {Current type}:{Criteria type}:{Criteria}:{Modifiers}:{Output Type}.  The data object representation may not look like this.

A Rule is evaluated by evaluating each subrule until a subrule evaluates as true.  A subrule evaluates as true if the {Criteria} combined with the {Modifiers} for {Criteria type} is satisfied and transitions a state from {Current Type} to {Output Type}.  Importantly, a subrule only applies to a `Cell` that is currently in {Current type}, and automatically evaluates as false if it isn't.  A subrule can be thought of logically as "Is a cell of {Current type} surrounded by enough cells of {Criteria type} conforming to the pattern defined by {Criteria} and {Modifiers} such that it should transition to {Output type}.  There is not restriction that the {Current type}, {Criteria type}, and {Output type} be different.

All `Rule`s have an implicit base rule where if none of the subrules evaluate as true, the `Cell` becomes an "inert" type.  The possible {Criteria} and {Modifiers} are specific to each dimensional and are described below.

All rules should be validated before computation to determine if they are valid and unit tests should be written to validate logic.

#### 1 Dimensional Rules
A 1D rule has a {Criteria} defined by the commonly used Wolfram code/notation, which, for example, is a simple number between 0 and 255 for a neighborhood distance of 1 (i.e. the cell and its neighbor on each side).  In general the number ranges from 0 to 2^b, where b = 2n+1, where n is the neighborhood size (see neighborhood modifier section below).  The number, represented in binary, defines for which neighborhood patterns the criteria applies.  For example, in a case where the n=1 there are 8 possible patterns each associated with one of the bits in the binary representation of the rule number: {(2^7, xxx), (2^6, xxo), (2^5, xox), (2^4, xoo), (2^3, oxx), (2^2, oxo), (2^1, oox), (2^0, ooo)} where x's represent a cell with type {Criteria type} and o's represent a cell with type o which can be anything except {Criteria type}. If the rule number was 30 (00011110 in binary), the patterns at indices 3-6 above would evaluate as true and patterns at indices 1-2 and 7 would evaluate as false.

These patterns can be generated algorithmically by constructing a binary tree of depth 2n+1 with n again being the neighborhood size with left branches being 1 and right branches being 0.  Walking down the tree depth first (left before right) and recording the branch values produces the pattern with 1's representing x's and 0's representing o's. 

In 1D a Criteria can be subject to a two modifiers:

**Randomness**

A randomness modifier is a simple modifier that is defined as {r:value} where the value represents a percentage in decimal form.  When a subrule with a randomness modifier is evaluated a random number between 0 and 1 is generated and if that number is larger than or equal to the "value" the rule is evaluated as true.  Importantly, like all modifiers, this is applied after the Criteria is applied.  For example, if the Criteria evaluates to false the result of the randomness modifier does not matter.

**Neighborhood Size**

A neighborhood size determines how many neighborhoods on either side of a `Cell` a Criteria checks. It is defined as {n:value} where the value is value is the neighbors on either side that the Criteria will check.  This value must be >=1.  A value of 1 would mean that the current `Cell` checks its immediate left and right neighbors.  Importantly, this value changes how the Wolfram code is interpreted since a neighborhood of n=1 has values 0-255 and for n=2 there are values 0-1023

#### 2 Dimensional Rules

A 2D rule's {Criteria} is a bit different compared to a 1D rule.  In two dimensions we do not care about the exact pattern, but instead just about how many other `Cell`s of type {Criteria type} surround the current `Cell` subject to the {Modifiers}.  The {Criteria} will always be a number representing a threshold.  If there are an equal or greater number of `Cell`s of {Criteria type} surrounding the `Cell` the {Criteria} will evaluate to true.  For example, if the rule was {TypeA}:{TypeB}:{4}:{TypeB} then any `Cell` that is currently TypeA would transition to TypeB if surrounded by at least 4 other TypeB `Cell`s.  This example excludes {Modifers} for simplicity.

There are several modifiers that 2D rules can have.

**Range**

The range modifier determines how far in each cardinal direction the `Cell` needs to look for neighbors.  The format is {n=value} were the value defines a square region of (2n+1)*(2n+1) `Cell`s centered on the current `Cell`.  This region may be further modified by the Neighborhood type below, but this square defines the maximal area.

**Neighborhood type**

The neighborhood type modifier changes what subset of the bounding square defined above in the Range modifer section.  The format is {neighborhood=value} where value can take one of three possible values:
* Moore: The neighborhood where all `Cell`s in the bounding square are included
* von Neumann: The neighborhood where only the `Cell`s in the 4 cardinal directions are used (up, down, left, right)
* Langdon: The neighborhood where only the diagonal neighboring `Cell`s are used.

**Symmetry**

TODO this modifier is not part of the initial implementation.

**Randomness**

The randomness modifier for 2D is equivalent to the modifier for 1D and has the same format of {r=value} where the value represents a percentage in decimal form.  When a subrule with a randomness modifier is evaluated a random number between 0 and 1 is generated and if that number is larger than or equal to the "value" the rule is evaluated as true.  Importantly, like all modifiers, this is applied after the Criteria is applied.  For example, if the Criteria evaluates to false the result of the randomness modifier does not matter.

## Binary Design

For our initial implementation, focus on the library functionality and the associated unit tests, but we do want a simple command line interface in the binary crate.  It should be able to setup and run a calculation, save the results/config, and maybe even visualize the calculation as it runs through a simple GUI.  A good initial set of conditions would be those associated with Conway's Game of Life in this new rule framework.

## Implementation Hints

When updating the `Grid`, each `Cell` is independent of the updates of its neighbors. That means you could using parallel process to update multiple `Cell`s at a time, you would need to make sure to update the `Grid` in a cloned `Grid` object.  This would be similar to double buffering in graphics.

Part of the calculation setup should involve a set `Grid` size so that there are some bounds.  The boundaries should be treated at the special "Inert" type.