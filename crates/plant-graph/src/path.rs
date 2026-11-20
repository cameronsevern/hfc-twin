//! Paths between any two points in the tree, in either direction. The sim's
//! `transfer(port_a, port_b, f_mhz)` (§11) reads its losses from these segments, including the
//! off-path routes through tap isolation that §5.1 needs.

use schema::common::ElementId;
use schema::fault::Location;
use schema::plant::{Edge, EdgeKind, Element, Port};

use crate::graph::{Anchor, PlantGraph, QueryError, edge_len_ft};

/// Which way a path crosses an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathDirection {
    /// Away from the root.
    Downstream,
    /// Toward the root.
    Upstream,
}

impl PathDirection {
    fn flipped(self) -> Self {
        match self {
            PathDirection::Downstream => PathDirection::Upstream,
            PathDirection::Upstream => PathDirection::Downstream,
        }
    }
}

/// One step along a path.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment<'a> {
    /// Some or all of a cabled edge.
    Cable {
        /// The edge.
        edge: &'a Edge,
        /// How much of it the path covers, feet.
        len_ft: f64,
        /// Which way the path crosses it.
        direction: PathDirection,
    },
    /// A part mounted on a port, with no cable.
    Direct {
        /// The edge.
        edge: &'a Edge,
        /// Which way the path crosses it.
        direction: PathDirection,
    },
    /// Across an element from one port to another. The two ports' roles say which loss applies:
    /// `Input` to `Through` is insertion loss, `Input` to `TapPort` is tap loss, and between a
    /// `TapPort` and a `TapPort` or `Through` is tap isolation (§2.1, §5.1).
    Device {
        /// The element.
        element: &'a Element,
        /// The port the path enters by.
        in_port: &'a Port,
        /// The port the path leaves by.
        out_port: &'a Port,
    },
}

/// An ordered route between two points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path<'a> {
    /// The steps, in order.
    pub segments: Vec<Segment<'a>>,
}

impl<'a> Path<'a> {
    /// The total cable along the path, feet: the `d` of a cavity (§5.1).
    pub fn cable_len_ft(&self) -> f64 {
        let lengths = self.segments.iter().map(|segment| match segment {
            Segment::Cable { len_ft, .. } => *len_ft,
            _ => 0.0,
        });
        lengths.sum()
    }

    /// The same route the other way: reversed order, flipped directions, swapped ports.
    pub fn reversed(&self) -> Path<'a> {
        let segments = self.segments.iter().rev().map(|segment| match *segment {
            Segment::Cable {
                edge,
                len_ft,
                direction,
            } => Segment::Cable {
                edge,
                len_ft,
                direction: direction.flipped(),
            },
            Segment::Direct { edge, direction } => Segment::Direct {
                edge,
                direction: direction.flipped(),
            },
            Segment::Device {
                element,
                in_port,
                out_port,
            } => Segment::Device {
                element,
                in_port: out_port,
                out_port: in_port,
            },
        });
        Path {
            segments: segments.collect(),
        }
    }
}

/// One step toward the root. Positions are measured from the edge's upstream end.
#[derive(Clone, Copy, PartialEq)]
enum UpHop<'a> {
    Cable {
        edge: &'a Edge,
        from_ft: f64,
        to_ft: f64,
    },
    Direct {
        edge: &'a Edge,
    },
    Cross {
        element: &'a Element,
        in_port: &'a Port,
        out_port: &'a Port,
    },
}

impl<'a> UpHop<'a> {
    /// The segment for this hop, or `None` for a zero-length cable.
    fn segment(self, direction: PathDirection) -> Option<Segment<'a>> {
        match self {
            UpHop::Cable {
                edge,
                from_ft,
                to_ft,
            } => (from_ft > to_ft).then_some(Segment::Cable {
                edge,
                len_ft: from_ft - to_ft,
                direction,
            }),
            UpHop::Direct { edge } => Some(Segment::Direct { edge, direction }),
            UpHop::Cross {
                element,
                in_port,
                out_port,
            } => match direction {
                PathDirection::Upstream => Some(Segment::Device {
                    element,
                    in_port,
                    out_port,
                }),
                PathDirection::Downstream => Some(Segment::Device {
                    element,
                    in_port: out_port,
                    out_port: in_port,
                }),
            },
        }
    }
}

impl<'a> PlantGraph<'a> {
    /// The route between two locations, in either direction. Going by a port's own edge adds no
    /// `Device` segment; going by another port crosses the element.
    pub fn path_between(&self, a: &Location, b: &Location) -> Result<Path<'a>, QueryError> {
        self.path_between_anchors(self.resolve(a)?, self.resolve(b)?)
    }

    /// The route from the root's RF port to an element's `Input` port. Empty for the root.
    pub fn path_from_root(&self, id: &ElementId) -> Result<Path<'a>, QueryError> {
        let element = self.element(id)?;
        let (Some((_, edge)), Some(root_port)) = (self.parent(id)?, self.root_port(id)?) else {
            return Ok(Path::default());
        };
        self.path_between_anchors(
            Anchor::Port(self.root(), root_port),
            Anchor::Port(element, self.port(element, &edge.to.port)?),
        )
    }

    /// Both ends climb to the root, then the part they share is dropped. What's left is the
    /// tree's one route between them, joined across the element or edge where they meet.
    fn path_between_anchors(&self, a: Anchor<'a>, b: Anchor<'a>) -> Result<Path<'a>, QueryError> {
        let mut up_a = self.climb(a)?;
        let mut up_b = self.climb(b)?;
        let mut join = None;
        while let (Some(&hop_a), Some(&hop_b)) = (up_a.last(), up_b.last()) {
            if hop_a == hop_b {
                up_a.pop();
                up_b.pop();
                continue;
            }
            match (hop_a, hop_b) {
                // Both are on one edge, so only the stretch between them counts.
                (
                    UpHop::Cable {
                        edge: edge_a,
                        from_ft: from_a,
                        ..
                    },
                    UpHop::Cable {
                        edge: edge_b,
                        from_ft: from_b,
                        ..
                    },
                ) if edge_a.id == edge_b.id => {
                    up_a.pop();
                    up_b.pop();
                    if from_a > from_b {
                        up_a.push(UpHop::Cable {
                            edge: edge_a,
                            from_ft: from_a,
                            to_ft: from_b,
                        });
                    } else {
                        up_b.push(UpHop::Cable {
                            edge: edge_a,
                            from_ft: from_b,
                            to_ft: from_a,
                        });
                    }
                }
                // They reach one element by different ports, and the path crosses it there.
                (
                    UpHop::Cross {
                        element: element_a,
                        in_port: port_a,
                        ..
                    },
                    UpHop::Cross {
                        element: element_b,
                        in_port: port_b,
                        ..
                    },
                ) if element_a.id == element_b.id => {
                    up_a.pop();
                    up_b.pop();
                    join = Some(UpHop::Cross {
                        element: element_a,
                        in_port: port_a,
                        out_port: port_b,
                    });
                }
                _ => {}
            }
            break;
        }
        // Different RF ports on the root meet with no hop to pop, since the root has no input.
        if join.is_none() {
            let top_a = self.top_port(&up_a, a)?;
            let top_b = self.top_port(&up_b, b)?;
            if let (Some((element, port_a)), Some((_, port_b))) = (top_a, top_b)
                && port_a.id != port_b.id
            {
                join = Some(UpHop::Cross {
                    element,
                    in_port: port_a,
                    out_port: port_b,
                });
            }
        }

        let upstream = up_a.iter().chain(&join);
        let mut segments: Vec<_> = upstream
            .filter_map(|hop| hop.segment(PathDirection::Upstream))
            .collect();
        segments.extend(
            up_b.iter()
                .rev()
                .filter_map(|hop| hop.segment(PathDirection::Downstream)),
        );
        Ok(Path { segments })
    }

    /// Where a remaining climb tops out, or `None` if it's at a point on an edge.
    fn top_port(
        &self,
        hops: &[UpHop<'a>],
        start: Anchor<'a>,
    ) -> Result<Option<(&'a Element, &'a Port)>, QueryError> {
        match hops.last() {
            None => Ok(match start {
                Anchor::Port(element, port) => Some((element, port)),
                Anchor::Point(..) => None,
            }),
            Some(UpHop::Cross {
                element, out_port, ..
            }) => Ok(Some((element, out_port))),
            Some(UpHop::Cable { edge, .. } | UpHop::Direct { edge }) => {
                let element = self.element(&edge.from.element_id)?;
                Ok(Some((element, self.port(element, &edge.from.port)?)))
            }
        }
    }

    /// The hops from an anchor up to the root.
    fn climb(&self, start: Anchor<'a>) -> Result<Vec<UpHop<'a>>, QueryError> {
        let mut hops = Vec::new();
        let (mut element, mut port) = match start {
            Anchor::Port(element, port) => (element, port),
            Anchor::Point(edge, pos_ft) => {
                hops.push(UpHop::Cable {
                    edge,
                    from_ft: pos_ft,
                    to_ft: 0.0,
                });
                let element = self.element(&edge.from.element_id)?;
                (element, self.port(element, &edge.from.port)?)
            }
        };
        while let Some((parent, edge)) = self.parent(&element.id)? {
            let input = self.port(element, &edge.to.port)?;
            if port.id != input.id {
                hops.push(UpHop::Cross {
                    element,
                    in_port: port,
                    out_port: input,
                });
            }
            hops.push(match edge.kind {
                EdgeKind::Direct => UpHop::Direct { edge },
                _ => UpHop::Cable {
                    edge,
                    from_ft: edge_len_ft(edge).unwrap_or(0.0),
                    to_ft: 0.0,
                },
            });
            element = parent;
            port = self.port(parent, &edge.from.port)?;
        }
        Ok(hops)
    }
}
