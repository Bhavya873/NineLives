"""Nine Lives training tools (all physics comes from the Rust module `ninelives_sim`).

env.py             Gymnasium and SB3 adapters over the Rust BatchedSim
train.py           PPO training loop (python -m ninelives.train)
train_config.py    the training yaml config
curriculum.py      which stages and levels a run trains on
callbacks.py       progress, evaluation and checkpoint callbacks
rollout.py         runs a policy on fixed seeds
eval.py            fixed-seed evaluation report (python -m ninelives.eval)
compare.py         paired comparison against the M7 bar
export.py          policy and viewer checkpoint export
charts.py          README charts from results/
tune_pd.py         CMA-ES tuning of the PD gains
hover_experiment.py  the hover-hack reward experiment report
run_files.py       where a run keeps its files
sim_config.py      builds the sim's JSON config from configs/
provenance.py      hashes that tie a result to its settings
"""

__version__ = "0.1.0"
