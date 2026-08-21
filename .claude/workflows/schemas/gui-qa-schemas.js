export const VISION_FINDINGS_SCHEMA = {
  type: "object",
  properties: {
    section: {
      type: "string",
      enum: ["header_bar", "classic_panel", "souped_drawer", "clipper_tweaks", "sequencer"]
    },
    alignment_score: {
      type: "number",
      description: "Visual alignment score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True if section strictly complies with the design specification"
    },
    findings: {
      type: "array",
      items: {
        type: "object",
        properties: {
          element: { type: "string" },
          severity: { type: "string", enum: ["critical", "major", "minor"] },
          category: {
            type: "string",
            enum: ["alignment", "color", "geometry", "typography", "missing_feature"]
          },
          observed: { type: "string" },
          expected: { type: "string" },
          suggested_fix: { type: "string" }
        },
        required: ["element", "severity", "category", "observed", "expected", "suggested_fix"]
      }
    }
  },
  required: ["section", "alignment_score", "spec_compliance", "findings"]
};

export const SYNTHESIS_SCHEMA = {
  type: "object",
  properties: {
    overall_score: {
      type: "number",
      description: "Overall weighted visual quality score from 0 to 100"
    },
    spec_compliance: {
      type: "boolean",
      description: "True if all sections pass visual quality gate"
    },
    plan_path: {
      type: "string",
      description: "File path where refinement plan was written"
    },
    top_defects: {
      type: "array",
      items: {
        type: "object",
        properties: {
          section: { type: "string" },
          element: { type: "string" },
          severity: { type: "string" },
          summary: { type: "string" }
        },
        required: ["section", "element", "severity", "summary"]
      }
    }
  },
  required: ["overall_score", "spec_compliance", "plan_path", "top_defects"]
};
