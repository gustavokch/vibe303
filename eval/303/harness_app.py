import streamlit as st

st.title("Analog Tier Voicing Harness")
tier = st.sidebar.selectbox("Tier", ["Low", "High", "Ultra"])
st.write(f"Ready to evaluate {tier} tier.")
